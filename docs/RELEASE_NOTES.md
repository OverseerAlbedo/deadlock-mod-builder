# Deadlock Mod Builder 0.13.0-alpha.52

This update frees disk space, adds optional breast motion, keeps halos and hats from swinging like hair, adds an experimental color editor, and makes sure your mod replaces the model the game actually loads.

## Storage

- **Settings → Storage** shows how much space each project uses, including the working copies in the CSDK and model toolset folders. **Clean up all projects** removes data the builder can recreate; **Remove…** deletes everything of a project you no longer need. Data a saved or open project uses and built VPKs are kept.
- Old model preparations and backups are pruned automatically, and extracted original hero files are shared between projects of the same hero.

## Motion

- **Breast motion** (optional, off by default). Turn it on and choose the amount on **Physics → Automatic motion**. It gives a springy bounce that does not sag; the two breasts move together and cannot swing into each other. Models without breast bones get them added where the chest has breasts (Model preparation → **Keep or add breast bones**, then **Inspect again**).
- **Halos, hats, crowns and hairpins** stay on the head instead of moving like hair.
- Fixes for builds that failed on MMD-style rigs: ribbons or cape strips hanging from breast bones ("Native secondary fitting is incomplete") and tip bones beside their bone ("Native float32 precision collapses or reverses a cloth proxy face").

## Appearance

- **Adjust colors (experimental)** in **Appearance → Surface changes** shifts a material's hue, saturation and brightness, with a preview. Your image files are not changed.

## Character targets

- The character picker follows the installed game's hero definitions. **Rem** now targets `familiar_wip.vmdl`, the model the game loads; the older `familiar.vmdl` is marked *Not used in game*. Analyze warns when a project targets a model the game does not load.

## After updating

- Projects with breast motion: open **Physics**, check **Breast motion** and the amount, then **Analyze** and **Build**.
- Yvonne-style rigs that failed before: press **Inspect source** again in Model preparation, convert, click **Use prepared model**, then Analyze and Build.
- The amount of breast motion, rigid accessories and color edits still need an in-game check.
