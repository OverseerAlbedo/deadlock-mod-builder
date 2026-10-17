# Deadlock Mod Builder 0.13.0-beta.9

Better results for **MMD models** (PMX characters imported into Blender with mmd_tools) and more automatic-movement fixes, found while building a second version of a character on Violet. Everything from the earlier betas is still in; see `CHANGELOG.md`.

## What changed

- **Face details** (Appearance page): layers a model keeps invisible until an expression shows them, such as MMD blush and tears, are now hidden in game. Before, they were drawn solid and could turn the face red. Switch on the blush, the tears or other such layers to show them; a shown layer keeps the soft edges of its own image.
- **Textures of MMD models:** the base texture is always used. Before, the shine (sphere) map was taken as the main texture, so skin, hair and clothes looked wrong or had no texture.
- **Preparing again refreshes automatic texture picks.** Textures and tints you chose yourself are kept.
- **Head and neck colliders:** bangs, short hair and long hair stop at the head and neck instead of swinging into the face. They are sized to your character.
- **Hair buns stay still.** Pick another part type on the Physics tab to make a bun move. A bun that flung around because of invisible helper bones is fixed too.
- **Shorter long skirts** (ending around mid-thigh) move as long skirts instead of stiff cloth.
- **Floor-length sleeves** (sleeve or 袖 names, or long thin strips joined only to a cuff) swing as streamers instead of stiff cloth.

## Checked

- Checked in a match on October 17, 2026 with an MMD character on Violet: Face details hide the blush and tears, and show them when switched on.
- Not yet checked in a match: the head and neck colliders, still buns, the shorter-skirt and long-sleeve rules.

## After updating the app

- To get the texture fix and the Face details switches on an existing MMD project, prepare the model again. Everything else only needs Analyze and Build.
- Projects, materials, prepared models and themes are kept.
