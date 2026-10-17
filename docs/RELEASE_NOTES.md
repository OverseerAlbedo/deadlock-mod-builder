# Deadlock Mod Builder 0.13.0-beta.8

A major fix for **automatic movement** (Physics → moving parts), found while building a character on Violet. Everything from the earlier betas is still in, including the installed-mods repair for the game update of October 16, 2026; see `CHANGELOG.md`.

## What changed

- **Violet: cloth no longer freezes.** Violet's own animations stiffen her cloth by name when she starts or stops running, jumps, dashes or slides. A replacement's cloth had none of those names, so the game stiffened all of it and every moving part stood still for about two seconds. Builds now give the replacement cloth empty sets with the hero's names, so those animations stiffen nothing. This works for any hero whose own cloth uses such names.
- **Real gravity.** Automatic movement had almost no gravity in game: tails looked static and long cloth floated. It now uses the game's own gravity. The movement preview, Fine-tune and saved presets are unchanged, and the Fine-tune gravity slider now works in game.
- **Skirts stay out of the body.** Capsules along the thighs, shins and the back of the hips are sized to your character, and each skirt joint may move only as far as its fabric is from the body. A shorter skirt panel between long panels now moves with the rest of the skirt.
- **Streamers and long sleeves.** A streamer can no longer fold up on quick turns or stretch longer than it is when the hero starts moving.
- **Parts that should stay still do.** Parts set to Rigid, ears with the new **Ear motion** switch (Physics tab) turned off, and switched-off gun charms are no longer simulated, so they cannot bounce.
- **Better part types.** Long round ribbons and tassels move as streamers, both sides of a left/right pair get the same type, and ribbons over the chest are no longer taken for breasts.
- **Build fix.** Builds since beta 6 that use the game's compiled model could stop with "Missing or ambiguous generated bone" at the attachment step. Nothing wrong was packaged or installed.

## Checked

- Checked in a match on October 17, 2026 with a Violet replacement: the cloth no longer stands still when she starts or stops running, the skirt no longer shows through to the body when walking backwards, and the tail and short hair move well.
- Still to tune: very long cloth hanging from the arms ruffles up a bit too much.

## After updating the app

- Build your projects with automatic movement again to get these fixes. You do not need to prepare the model again.
- Projects, materials, prepared models and themes are kept.
