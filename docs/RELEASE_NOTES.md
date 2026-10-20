# Deadlock Mod Builder 0.13.0-beta.12

Fixes for automatic movement on small models and a clearer error for Replacement cloth setup (reported in issue #4). Everything from the earlier betas is still in; see `CHANGELOG.md`.

## Fixed

- **Automatic secondary motion** could stop with "static-anchor rod maxima and capsule exclusion cannot both be satisfied at declared rest pose". The app makes colliders for your model's head, neck, legs and hips so moving hair and cloth stop at the body. On some models a collider grew over the base of a moving part: a chibi's hair inside its large head, or boot flaps on the shins. Colliders now always leave room for it. On our test projects only one model's colliders changed; every other model keeps the same colliders as before.
- When that check fails, the error now names the collider, so a report needs no log.
- **Replacement cloth setup** with the game's own model file as the tested reference (for example Silver's `werewolf.vmdl_c`) stopped with "Compiled mesh inverse bind differs from model skeleton". That option needs a cloth setup made and tested for your replacement model, and the game's own models fail its checks (all nine heroes we tried). The error now says so and points to **Preserve original physics**, which keeps the hero's own hair and cloth. The checks themselves are unchanged.

## Checked

- Checked on the PC on October 20, 2026: the collider sizes of 12 test projects (11 unchanged), the Replacement cloth setup error with Silver's own model, and nine of the game's hero models refused as tested references with the new message.
- Not yet checked in a match: automatic movement on a small model with the new collider limit.

## After updating the app

- Nothing to redo. Projects, settings, prepared models and themes are kept. Builds compute the colliders again, so you do not need to click Analyze first.
