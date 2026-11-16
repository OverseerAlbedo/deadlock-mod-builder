# How to make a Deadlock character mod

This tutorial takes you from a character model to a mod in Deadlock. You end
up with a VPK in the game's `addons` folder that replaces a hero's model with
yours. Deadlock Mod Builder, an open-source Windows app, leads you through five
steps. Blender runs in the background to prepare the
model; no separate Python installation is needed.

This page is the quick path. The [model preparation guide](MODEL_PREPARATION.md)
explains every step in detail.

## What you need

- Windows 11 x64. Windows 10 x64 should work, but it is untested.
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
  The installer adds it if it is missing.
- These four tools:

  | Tool | Used for |
  | --- | --- |
  | Deadlock, installed through Steam | Choosing heroes, building and installing |
  | [Blender](https://www.blender.org/) 4.2–4.5, 5.0, 5.2 LTS (recommended) or 5.3 | Preparing Blender, FBX and glTF models |
  | [Source 2 Viewer CLI](https://github.com/ValveResourceFormat/ValveResourceFormat/releases) | Extracting and decompiling game assets |
  | [Reduced CSDK 13](https://deadlockmodding.pages.dev/modding-tools/csdk-13) | Compiling the materials and the hero model |

- A rigged humanoid character model: a Blender, FBX, glTF/GLB or prepared DMX
  file. Rigs with Mixamo, Rigify, DAZ, Character Creator, MMD or ValveBiped
  naming work, and so do unusual bone names. Bodies that are not humanoid,
  such as quadrupeds, still need a manual source copy (see
  [Limits](MODEL_PREPARATION.md#limits)).

The app does not include Valve tools, game files or third-party character
models: you bring your own model. The app runs Blender and the game's resource
tools on files you choose and writes to your Deadlock installation's `addons`
folder, so only open source files and projects you trust.

## Install and set up

1. Download `DeadlockModBuilder-v<version>-windows-x64-setup.exe` from the
   [Releases](https://github.com/OverseerAlbedo/deadlock-mod-builder/releases)
   page. Each release lists the installer's SHA-256.
2. Run the installer and start **Deadlock Mod Builder**.
3. On first start the new-user guide opens. Point the app at the four tools
   (**Detect tools** finds most of them), click **Save and check tools**, then
   **Test my setup**.

The switch in the top bar chooses Simple or Expert. Simple is the default and
shows only what a first mod needs. Choices you must make are never hidden.

## Step 1: Character & model

Pick the hero model to replace and your model file. The hero picker lists the
model the game loads under the hero's name, and marks roster models the game
does not load as *Not used in game*. Choose the main hero model unless you want
an alternate variant.

If your gun is in a separate file, you can also choose an optional **Gun from
another file** here.

A Blender, FBX or glTF model goes straight to step 2 and is prepared
automatically.

![Character & model step](images/character-and-model.png)
*Character & model: pick the hero to replace and your model file. The pictures on this page use a plain example model.*

## Step 2: Prepare model

This step fits your model to the hero's exact skeleton.
**Prepare automatically** inspects your file, applies the suggestions, checks
the result, converts it and uses the model in one go, then continues to step 3.
Your own file is never written.

Preparation usually finishes on its own. If it stops, it names what to decide,
such as a joint match or a shape-key choice, with a button to each spot. When
your model has shape keys, **Shape key handling** is a required choice.

## Step 3: Appearance & weapons

Check the gun fit in the 3D preview, review the materials and textures, and
accept them.

- The gun: if your model contains a gun, the **Weapon** tab offers to fit it
  onto the hero's gun, leave it out, or keep it in the hand. Using your gun
  instead of the hero's is recommended and preselected. The preview shows your
  gun over the hero's own. If the gun points backwards, use **Flip direction**;
  other buttons turn, tilt and move it. **Apply gun changes** prepares the model
  again in one click.
- Materials: textures are found in your model's materials. The app has a
  color editor, texture replacement and a review list before building.

![Gun fit preview](images/gun-fit.png)
*Appearance & weapons: the fit preview shows your gun over the hero's own, with buttons to turn, tilt and move it.*

## Step 4: Physics & preview

Hair, tails, ears, skirts, cloth and streamers can move on their own. On the
**Physics** tab, switch moving parts on or off and fine-tune their movement.

**Physics → Your character → Movement** plays the hero's own idle, run and jump
clips on your character. It approximates the game's cloth solver, for tuning by
eye.

![Physics & preview step](images/physics-and-preview.png)
*Physics & preview: look at your prepared character in 3D and choose how its moving parts move.*

## Step 5: Build & install

Analyze, build and install. The app inspects the project, compiles the model
and materials, packs the VPK and installs it.

Before you build, look at the mesh mapping on this page. Your meshes are named
after your source (Body, Face), not after the hero's render nodes, so the page
offers a **suggested mapping**. Nothing is applied until you choose it. Source
meshes that no render node uses would be missing in game; a warning names them,
with a button that adds them to the main node.

If a check blocks the build, fix the reported input. Bypassing a check produces
a broken or unchanged mod. Analyze also reports a VPK slot another mod uses.

![Build & install step](images/build-and-install.png)
*Build & install: the project checklist before building.*

Projects are saved as `.dmb.json` files.

## Check it in the game

Check the character in Deadlock. The Build & install page lists what to look at
in game. A successful build and the previews do not replace a look at the
character in a match. Look at:

- the grip, aim and muzzle position (shots and muzzle flashes come from the
  original muzzle bones);
- elbow and knee bending;
- clipping;
- for heroes with a gun in each hand, the two-gun animations;
- how the moving parts move.

## If something goes wrong

- **A path is red in Settings:** use the **Use …** fix it offers, or **Fix all
  marked paths**, then **Test my setup**.
  See [Setup](TROUBLESHOOTING.md#setup).
- **Preparation stops:** the Prepare model step names the object or joint and
  offers a button to it. Rigs driven by live controls may need a baked export.
  See [Models and mapping](TROUBLESHOOTING.md#models-and-mapping).
- **The gun points backwards or is upside down:** flip or roll it in the fit
  preview, then check aim and grip in game. See
  [Guns](TROUBLESHOOTING.md#guns).
- **Wrong model in game:** check the hero's model path, the VPK slot and
  conflicting mods. See [Guns](TROUBLESHOOTING.md#guns), which also covers
  choosing the hero's main model.
- **Movement differs in game:** the preview approximates the game's cloth;
  adjust the Fine-tune sliders, analyze and build again. See
  [Physics](TROUBLESHOOTING.md#physics).
- **You need help:** use **Logs** and **Step timings** to find the failing
  stage, then report it with the text from **Copy for support**. See
  [Logs](TROUBLESHOOTING.md#logs).

## Next steps

- [Simple and Expert](MODEL_PREPARATION.md#simple-and-expert): what the
  Expert mode shows.
- [Guns](MODEL_PREPARATION.md#guns): heroes with two guns, a gun from another
  file, and gun charms.
- [Moving parts](MODEL_PREPARATION.md#moving-parts) and
  [the movement preview](MODEL_PREPARATION.md#fine-tune-and-the-movement-preview):
  part types, Fine-tune sliders and saved presets.
- [Hero colors](MODEL_PREPARATION.md#hero-colors): recolor the hero's own
  clothes without a replacement model.
- [Troubleshooting](TROUBLESHOOTING.md): compatibility limits and fixes for
  common failures.
