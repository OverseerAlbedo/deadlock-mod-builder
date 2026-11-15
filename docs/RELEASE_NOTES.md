# Deadlock Mod Builder 0.13.0-beta.15

**Reduced CSDK 13 by default**, a simpler setup, **Notifications** in the sidebar, Blender 5.3, Source 2 Viewer 21.1 and a fix for long file names. Everything from the earlier betas is still in; see `CHANGELOG.md`.

## New

- **Reduced CSDK 13** is supported and is now the default. It compiles the materials and the hero model, so no separate model compiler is needed. A new setup needs four things: Deadlock, Blender, Source 2 Viewer CLI and Reduced CSDK 13. Then click **Detect tools**, **Save and check tools** and **Test my setup**.
- **Notifications** in the sidebar, above the version. Errors and the results of Analyze, Prepare model, Build, Install and Test my setup are listed there with a green, yellow or red badge; click one to open it. **Clear** empties the list for now. An error now shows one line at the top of the page, and **Show details** opens the whole error with **Copy for support**.
- **Blender 5.3** is accepted. Blender 5.2 LTS stays the recommended version, and the Blender path is now in the main list of Settings.
- **Source 2 Viewer CLI 21.1** works with the app.

## If you already use Reduced CSDK 12 or a downloaded model toolset

- Nothing switches by itself, and your setup keeps building.
- With Reduced CSDK 12 chosen, Notifications shows **Reduced CSDK 12 in use** with the steps to move to Reduced CSDK 13 and a **Find Reduced CSDK 13** button. **Ignore** hides it for good if you want to keep your setup (Settings can show it again).
- A setup that uses a downloaded model toolset gets one button in Settings, **Compile models with Reduced CSDK 13**, once Reduced CSDK 13 is the Community SDK. The toolset page and its fields are now under Settings → Advanced configuration → Downloaded model toolset (older setups).

## Fixed

- Automatic preparation of an FBX model stopped with "The system cannot find the path specified. (os error 3)" when a folder below the model held a picture with a very long name (issue #7). Only the images the FBX uses are copied now, long paths no longer break the copy, and a texture whose path would be too long for Blender gets a message naming the file.
- Blender closed without an error while inspecting a model whose armature has an object-level driver.
- Detect tools no longer picks a Counter-Strike tools folder as the model compiler, finds the `Reduced_CSDK_13` folder the ZIP extracts to, and chooses among installed Blender versions by version number.

## After updating the app

- Projects, settings and themes are kept.
