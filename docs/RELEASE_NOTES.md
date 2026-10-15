# Deadlock Mod Builder 0.13.0-beta.4

The fourth beta is the first release from the cleaned-up public repository. The data captured from third-party characters and the builder's own compiled Vindicta references moved out of the source tree into a private assets repository, and the app handles their absence on its own. Nothing changes for an ordinary project. Everything from beta 3 (themes and the customizer, the movement preview, the redrawn sliders, the build-page fixes) is still in; see `CHANGELOG.md`.

## What changed

- **Physics & preview → Automatic motion** offers **Automatic** only in builds made without the private assets, which includes every installer published on GitHub. A project saved with a TangTang preset still shows its choice, marked *not included in this build*, and Analyze explains how to switch to Automatic.
- The included **Vindicta balanced cloth** choice is hidden in the same builds. A tested compiled model you pick yourself still works as before.
- The notes of every published installer end with **TangTang presets: included** or **not included**, so you can tell which build you have.

## Checked in game

- Shiv and Wraith with Keep source proportions look right. Haze and Lady Geist projects compile with heroes decompiled by the newer Source 2 Viewer and the game's cloth is restored.
- Not yet checked in game: the movement amounts from Fine-tune, the source spring values, and the finger bend, charm swing and dual-gun aim from beta 1. Please report what you see.

## After updating

- Open your project and run **Analyze project** once. Material choices, prepared models and themes are kept.
- If your project used a TangTang preset, choose **Automatic** under Physics & preview → Automatic motion before building.
