# Deadlock Mod Builder 0.13.0-beta.14

**Feet on the floor**, better automatic movement for long capes, braids and arm cloth, gun tilt and aim, and VPK slot fixes. Everything from the earlier betas is still in; see `CHANGELOG.md`.

## New

- **Feet on the floor** (Model preparation → Fit & review). The fit lines up your model's ankles with the hero's, so feet taller than the hero's (heels, thick soles) sank into the floor, by 1.3–5.6 in on most test characters. Choose **Line up the ankles** (the default, the fit as before), **Shorten or lengthen the legs** (below the hips; the knees and ankles then sit a little off the hero's) or **Move the whole character up or down** (the body and hands then sit off the hero's joints). Prepare model says how far the soles were from the floor. The converted model's 3D view now draws its grid at the game's floor, so a gun hanging below the feet no longer makes the character look like it floats.
- **Gun tilt and aim** (Model preparation → Gun): pitch (muzzle up or down) and yaw (left or right) around the grip besides roll, with 5° buttons and exact fields under Advanced, for each hand.
- **Arm colliders** for automatic movement: cloth hanging from the shoulders stops at the arms instead of passing through them, and capes hanging from an arm follow it the way Valve's own arm cloth does.
- **Long capes and cloak panels** inside a cloth part now swing like streamers (twist protection, fold limits and tethers) instead of hanging stiff, **braids** move as hair, and **hair reaching past the knees** hangs under gravity. Other characters' moving parts are unchanged.

## Fixed

- A gun from its own file whose left and right halves are named for the hands was split into two guns (one-gun heroes got only half). It stays one gun now.
- Analyze stopped with "Preview cannot triangulate a self-intersecting polygon" or "Preview cannot triangulate this DMX polygon" on guns with flat folded faces or faces whose corners sit at one spot. Those faces are accepted now; faces that really cross themselves still get help.
- Build stopped with "Missing or ambiguous generated bone" when moving-part bones had names that differ only in non-English characters (for example 侧裙, 前裙, 后裙). They get distinct names now; such models need Convert and Use prepared model once (Analyze says so).
- Capes hanging from a breast bone (MMD-style rigs) moved as breasts. They move as the cape now.
- **VPK slots**: a slot stayed "in use" after you deleted that VPK while the app was open, and Build was blocked by "Confirm replacement of the existing output" when rebuilding into the same slot. Find free slot reads the addons folder again, the Build page refreshes every few seconds, your own installed build is recognized, and Build replaces its own earlier output (kept as a backup).
- Adding a gun file whose parts have modifiers could stop with a UnicodeDecodeError.
- "Invalid compiled mesh remapping table" (issue #6) now says which mesh and bone are affected. Mesh mapping shows **Add it to …** for unused meshes even while a suggestion is waiting.

## Checked

- Checked in a match on October 24–25, 2026 with an MMD-style character on Vindicta: the braid, back cloak and shoulder capes move (a little clipping on the arm remains), and Shorten or lengthen the legs is hardly noticeable.
- Not yet checked in a match: Move the whole character up or down, and gun tilt and aim.

## After updating the app

- Characters prepared earlier keep their fit. To use Feet on the floor, choose it and click Prepare model again. Projects, settings and themes are kept.
