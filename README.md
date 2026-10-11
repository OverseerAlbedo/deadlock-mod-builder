# Deadlock Mod Builder

A Windows desktop application for creating Deadlock character replacement mods. Prepare a custom model, map it to a hero, review materials and movement, compile a VPK, and install it from one interface.

The application uses **Tauri, React, TypeScript, and Rust**. Blender performs optional model conversion through its own Python interpreter. A separate Python installation is not needed to run the app.

![Physics workspace with an example project](docs/images/physics-workspace.png)
*Current Physics components rendered with sample project data for UI review. This screenshot is not an in-game simulation or a Windows startup test.*

## Features

- Hero selection with model paths and separate entries for alternate models.
- Drag-and-drop model import, prepared-DMX projects, and humanoid conversion.
- Automatic body-role and weighted-bone suggestions, common rig naming conventions, structural recognition for rigs with generic or unusual bone names, automatic upright rotation, qualified recovery of disconnected rest skeletons, mapping previews, source-proportion preservation, and review before conversion.
- Character isolation: cameras, lights, hidden alternates, other characters, scenery and helper objects are set aside automatically; common mesh problems (generative modifiers, instanced or mirrored meshes, missing weights or materials, bone-parented props) are repaired in the workspace copy. See [model preparation](docs/MODEL_PREPARATION.md).
- Gun customization: fit the gun from your character file onto the hero's own gun (matching its grip, barrel and size and following its weapon bones), leave it out, or build a separate weapon model from the same file.
- Material and image previews, texture recovery, image fitting, and **Accept all** for resolved image choices.
- Automatic secondary motion for supported hair, tails, ears, streamers, skirts, and clothing. Includes twist protection, long-hair softening, stretch restraints, connected garment fitting, and upper-attachment limits.
- A qualified Vindicta cloth reference and preserved character-specific setups.
- Model preview and supported motion/animation inspection. Live game physics must still be checked in Deadlock.
- Extras with icon previews and replacement across ordinary Panorama image formats, including padded icons and fallback-image metadata; audio playback, drag-and-drop sound replacement, and **0–200%** replacement volume.
- Integrity checks, Source 2 compilation, VPK packaging, and validated installation.
- Cancelable operations, build progress, step timings, complete tool logs, and guided settings.

Compatibility depends on the input model, game resource format, and configured tools. See [limitations and troubleshooting](docs/TROUBLESHOOTING.md).

## Installation

When a release has been published, open this repository's **Releases** page and download one Windows x64 asset:

| Asset | Use |
| --- | --- |
| `DeadlockModBuilder-v<version>-windows-x64-setup.exe` | Run the installer, then launch Deadlock Mod Builder. |
| `DeadlockModBuilder-v<version>-windows-x64-portable.zip` | Extract the entire ZIP and run `DeadlockModBuilder/deadlock-mod-builder.exe`. Keep its resource folders alongside it. |

The ZIP build does not require installation, but settings and workspaces still use the normal Windows user-profile locations. It is not a self-contained USB profile. Both builds require Microsoft Edge WebView2 Runtime. The installer can obtain it; portable users must have it installed.

If no release is published yet, use [Build from source](#build-from-source). Successful CI runs also provide temporary build artifacts under **Actions**; those are not published releases.

On first launch, open **Settings**, configure the game and tools, save the settings, and run the setup checks. The onboarding and model-toolset guides are available there.

## Requirements

| Requirement | When needed | Included? |
| --- | --- | --- |
| Windows 11 x64 | Tested user environment | No |
| Microsoft Edge WebView2 Runtime | Desktop interface | Installer can obtain it |
| Deadlock installation and original game archive | Select a hero and build/install a mod | No |
| Compatible Source 2 model compiler and material compiler | Native compilation | No |
| Source2Viewer CLI | Configured extraction/decompilation workflows | No |
| Blender **4.2–4.5, 5.0, or 5.2 LTS** | Humanoid preparation and supported import/conversion operations | No |
| Full-precision cloth reader | Independent native-resource verification | Yes, self-contained Windows executable |
| Qualified physics references | Included-reference workflow | Yes |

Blender versions listed above are accepted by the current bridge. A successful version check does not certify every rig, add-on, constraint, or file written by a newer Blender version. Prepared-DMX workflows can avoid Blender conversion.

Source 2 model and material compilers can require different toolsets. Follow the app's setup checks and toolset guide; do not mix arbitrary compiler DLL versions. No CSDK, Valve compiler, game installation, or third-party character model is included in the application installer.

## First project

1. **Settings:** configure Deadlock, its original archive, the compilation tools, a workspace, and Blender if needed. Save and check the setup.
2. **Overview:** create/name a project and choose the hero and exact model to replace. Save the project.
3. **Import a model:** choose a source file or drop it into the app.
4. **Choose preparation:** use an already prepared DMX with the required hero skeleton, or inspect a humanoid source in **Model preparation**. Check **Scene objects** (what was kept or set aside), then review body roles, moving parts, bone mapping, the **Weapon** tab if the file contains a gun, and fit before converting. Use source-proportion preservation when the source should retain its shape.
5. **Appearance:** review mesh replacements, materials, and images. Resolve ambiguous choices before accepting them.
6. **Physics / Motion / Extras:** choose the supported movement setup and optional replacements. Automatic physics uses the secondary groups preserved during preparation; review its report.
7. **Review & build:** analyze the current project, complete required checks, and build the VPK. Check the preview and use the output/log controls if a stage fails.
8. **Install and test:** use the validated installation action, then inspect the character in Deadlock. Check movement, jumping, dodging, clipping, and the selected model variant.

A prepared project already has the skeleton/mapping needed by the target workflow. Humanoid conversion creates that prepared result from a source rig. An unrigged model requires the preparation/weighting workflow; selecting a file does not make arbitrary geometry automatically game-ready.

## Build from source

For Windows development, install:

- Node.js **24** with npm (selected by `.node-version`).
- Rust **1.99.0** with the **MSVC** target (selected by `rust-toolchain.toml`).
- Microsoft Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK.
- Microsoft Edge WebView2 Runtime.
- PowerShell 7 for the release/check scripts.

From the repository root:

```powershell
npm ci
npm test -- --maxWorkers=2
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked
npm run tauri -- dev
```

`npm run dev` opens only the browser frontend; native model operations require `npm run tauri -- dev`.

For the same Windows tests, portable ZIP, and startup check used by push CI:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

`Build-Windows.cmd` invokes that script. Add `-Release` only when intentionally building a requested installer. Ordinary pushes create portable test artifacts without generating a release installer. Outputs are in `artifacts/release/`; diagnostic logs are in `artifacts/validation/`. This command builds local files and does **not** publish a GitHub release. See [development](docs/DEVELOPMENT.md) and [release procedure](docs/RELEASING.md).

## Project structure

| Path | Purpose |
| --- | --- |
| `src/` | React interface, previews, and frontend tests |
| `src-tauri/` | Tauri desktop host, Rust pipeline crates, and regression fixtures |
| `src-tauri/crates/deadlock-preparation/resources/blender/` | Active Python scripts embedded for Blender |
| `resources/` | Hero catalog, tool recipe, and hash-pinned native readers |
| `physics-references/` | Required qualified references; not a game dump |
| `tools/cloth-reader/` | Source and locked dependencies for the independent reader |
| `scripts/` | Repository checks, Windows packaging, startup check, and optional Blender probes |
| `legacy/python/` | Minimal reference helpers used by Blender integration tests; no retired GUI |
| `.github/workflows/` | Read-only test/build CI |
| `docs/` | Usage, development, release, troubleshooting and [model preparation](docs/MODEL_PREPARATION.md) guidance |
| `licenses/` | Third-party notices |

The retired Python desktop app, old installers/wheels, migration checkpoints, and bulky historical logs are not part of this repository layout. Active Blender scripts, the reader's rebuild sources, runtime resources, and Rust/React tests remain included.

## Troubleshooting

- **Blender not configured:** select its actual executable in Settings, save, and validate it. The supported version alone does not establish input compatibility.
- **Import or mapping fails:** review the named object/bone and the full log. Ambiguous rigs and unsupported live controls may require a baked/exported source.
- **Compiler missing or fails:** recheck the separate model/material tool paths and their complete toolsets. Save full stdout/stderr from Logs.
- **Validation blocks a build:** fix the reported input or mapping issue; bypassing integrity checks can produce a corrupt or unchanged mod.
- **Wrong character/model in game:** check the exact hero model path, installed VPK slot, and conflicting mods.
- **Physics differs in game:** preserve the prepared secondary groups, select the intended Physics mode, analyze again, then rebuild. Automatic classification does not guarantee every unfamiliar garment.

More detail: [troubleshooting and compatibility](docs/TROUBLESHOOTING.md).

## Contributing and licensing

See [CONTRIBUTING.md](CONTRIBUTING.md). Keep changes focused and preserve the accepted Vindicta, Daki, TangTang, and Graves workflows. Report the app version, source format, selected hero/model, reproduction steps, and relevant logs.

Application source is covered by [MIT](LICENSE). Third-party components retain their own licenses; see [third-party and asset notes](docs/THIRD_PARTY.md). The source license does not grant rights to Valve assets or user models/textures.

This is an unofficial third-party tool, not affiliated with or endorsed by Valve. Deadlock and its game assets belong to their respective owners.
