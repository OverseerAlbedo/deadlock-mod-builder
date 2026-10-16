# Deadlock Mod Builder 0.13.0-beta.5

A small release for Violet, who joined the game on October 16, 2026. Everything from beta 3 and beta 4 is still in; see `CHANGELOG.md`.

## What changed

- **Violet** is offered as a playable hero in Character & model. She carries no gun mesh (her brush is part of the rig), so the Guns step treats her like Baba, Lash or Mina, and her cloth comes from Automatic motion in a replacement.
- Whether a hero can be picked in a match now comes from the installed game's own hero definitions. **Nurse Harrow** still shows *Not playable yet* and switches to *In game* on her own once the game releases her (October 20); no app update is needed for that.

## Checked against the October 16 game files

- Violet's model passes the compile template preparation, and all 44 heroes pass the gun-reference and holding-pose check.
- Not yet checked in game: a finished Violet replacement. Please report what you see.

## After updating

- Open your project and run **Analyze project** once. Material choices, prepared models and themes are kept.
