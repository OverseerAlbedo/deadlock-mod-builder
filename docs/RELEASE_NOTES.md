# Deadlock Mod Builder 0.13.0-beta.2

The second beta fixes the builds that stopped working with the October 2026 game files, makes the setup check and fix itself, adds a Simple interface, a movement preview with fine-tuning sliders, autosave, and clearer errors. Everything new is listed in `CHANGELOG.md`.

## Fixes for the October 2026 game files

- Build stopped with “ModelDoc28 cannot mix modern Softbody with legacy cloth containers” (Haze) or “Compile template bone/hierarchy does not match original hero: $cloth_m0p0 (bone is missing from the source skeleton)” (Yamato, Lady Geist, Wraith, Abrams, Paradox, Rem and eleven other heroes) when the hero was decompiled with a newer Source 2 Viewer. The build now lists the game's cloth bones itself, takes bone positions from the game's model where the decompile drifts, and restores the game's cloth after compiling as before. No need to decompile again.
- Baba can be chosen as the hero again (“unknown KV3 type 27”).
- The hero picker marks each model: **In game**, **Alternate**, **Not playable yet** (Nurse Harrow, Violet) or **In development**.

## Setup that checks itself

- Every path in Settings and the new-user guide is checked as you type, with a one-click **Use …** button when the right path is nearby and **Fix all marked paths**. **Test my setup** compiles one small test material and asks the model compiler what it reads (about 10 seconds).
- Detect tools no longer puts the Community SDK into the model compiler fields (its builds always failed with a schema mismatch); the model compiler setup selects and saves the toolset itself when a download or verification passes.
- Errors from setup, preparation and Build say what went wrong in plain words, with numbered steps and a button to the page where the fix happens. **Copy for support** hides your Windows user name, as do the logs.

## Simple interface and a calmer flow

- **Simple / Expert** switch in the top bar (Simple by default). Simple shows the five steps with recommended settings; Expert shows everything.
- Prepare model collapses matched joints and bones into one line, has one 3D view (*Your model*, *Joint check*, *Result*) and keeps rare fit options under **Customize fit**. Build & install groups its settings under **Customize build** and shows one **Check in game** list after a build.
- Choosing a character asks whether the gun is in a separate file before preparation starts. Dual-wield heroes get a **Both guns** control. The gun fit preview shows the hero in its holding pose.
- A progress bar with the time left, a green **Completed** after compiling, Ctrl+B to build and Ctrl+1…5 for the steps, and a list of what changed since the last build.

## Movement

- **Movement preview** (Physics → Your character → Movement): the prepared character plays the hero's stand, run and jump with hair, cloth, tails and other parts swinging; speeds 1× to 0.25×. It approximates the game's cloth for tuning by eye.
- **Fine-tune movement**: Stiffness, Damping, Gravity and Swing range per part type (hair, tails, streamers, ears, cloth, skirts, accessories, breasts, butt, gun charms), saved presets that fit any model, and optionally your model's own VRM/MMD spring values as the starting point.
- Butt bones are recognised separately from breasts and stay still unless **Butt motion** is on. Long hair strands and thin tails move instead of staying stiff; hanging strips on breast bones no longer flap through the body.
- **Shiv and Wraith** with Keep source proportions: the torso, legs and arms follow the hero's proportions without stretching one limb.

## Also

- Autosave every 3 minutes with recovery after a crash.
- Chinese, Japanese and Korean material names are spelled in Latin letters in prepared names and logs.
- Build warns when Deadlock is running, the workspace drive is nearly full or a tool sits inside OneDrive, and no longer says “Another operation is running” after the pre-build warning.

## Checked in game

- Checked on this PC: Haze and Lady Geist projects compile with heroes decompiled by the newer Source 2 Viewer and the game's cloth is restored; Yamato's new decompile passes the compile template check.
- Not yet checked in game: the movement amounts from Fine-tune (hair, tails, butt, breasts), the source spring values, Shiv/Wraith proportions, and the finger bend, charm swing and dual-gun aim from beta 1. Please report what you see.

## After updating

- Open your project and run **Analyze** once on the Physics page (the movement preview and fine-tuning need it) and **Prepare model → Prepare automatically** when you want the new material names or the holding pose in the gun preview. Material choices are kept.
