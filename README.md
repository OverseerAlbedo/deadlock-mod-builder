# Deadlock Mod Builder

[![Windows tests and portable application](https://github.com/OverseerAlbedo/deadlock-mod-builder/actions/workflows/windows.yml/badge.svg)](https://github.com/OverseerAlbedo/deadlock-mod-builder/actions/workflows/windows.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Turn a character model into a Deadlock hero replacement mod: pick the hero, drop in your model, review the guns, materials and movement, then build and install a VPK, all from one desktop application.

Built with **Tauri, Rust, React and TypeScript**. Blender runs in the background as a geometry engine for model preparation; no separate Python installation is needed.

![Physics workspace with an example project](docs/images/physics-workspace.png)
*The Physics step with sample project data. UI capture, not an in-game simulation.*

## Table of contents

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Documentation](#documentation)
- [Building from source](#building-from-source)
- [Project structure](#project-structure)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Five guided steps:** Character & model → Prepare model → Appearance & weapons → Physics & preview → Build & install, with automatic preparation of Blender, FBX and glTF models and plain-language explanations at every stop.
- **Works with real-world rigs:** common naming conventions (Mixamo, Rigify, DAZ, Character Creator, MMD, ValveBiped), rigs with unusual bone names, cluttered scenes (cameras, lights, scenery, other characters), game rips and models with shape keys. See [model preparation](docs/MODEL_PREPARATION.md).
- **Guns:** replace the hero's gun with the one in your model or in a separate gun file, including dual-wield heroes (one gun per hand), gun charms that swing, a live 3D fit preview with simple adjustment buttons, and separate weapon models built into the same mod.
- **Materials:** textures recovered from the model's materials, baked texture atlases, decals, a color editor and texture replacement, with a review list before building.
- **Movement:** automatic secondary motion for hair, tails, ears, skirts, cloth, streamers and gun charms, optional breast motion, rigid accessories, and qualified physics references.
- **Extras:** hero icons, ability images and sounds, with previews and drag-and-drop replacement.
- **Trustworthy builds:** every stage is hash-verified, compiled with the official Source 2 toolset, validated against the installed game, packaged as a VPK and installed in place, with complete tool logs and step timings.

Compatibility depends on the input model, the game's current resource formats and the configured tools. See [troubleshooting](docs/TROUBLESHOOTING.md).

## Requirements

| Requirement | Needed for | Included |
| --- | --- | --- |
| Windows 11 x64 | Running the app | — |
| [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) | The interface (installed on current Windows) | Installer can add it |
| Deadlock installation | Choosing heroes, building and installing | No |
| Source 2 model and material compilers (Reduced CSDK) | Compiling | No |
| Source2Viewer CLI | Extracting and decompiling game assets | No |
| Blender 4.2–4.5, 5.0 or 5.2 LTS | Preparing Blender, FBX and glTF models | No |
| Cloth reader, physics references | Independent verification of movement setups | Yes |

The app does not include Valve tools, game files or third-party character models. **Setup** inside the app checks every configured tool and explains what is missing.

## Installation

Download one Windows x64 asset from the [Releases](https://github.com/OverseerAlbedo/deadlock-mod-builder/releases) page:

| Asset | Use |
| --- | --- |
| `DeadlockModBuilder-v<version>-windows-x64-setup.exe` | Run the installer, then start Deadlock Mod Builder. |
| `DeadlockModBuilder-v<version>-windows-x64-portable.zip` | Extract the whole ZIP and run `DeadlockModBuilder\deadlock-mod-builder.exe`. Keep the folders next to it. |

Settings and workspaces live in the normal Windows user profile for both builds. Current releases are pre-releases: they are tested on the developer's machine and in CI, and in-game checks are listed in the [release notes](docs/RELEASE_NOTES.md).

## Quick start

1. **Setup** (first start): point the app at Deadlock, the Source 2 compilers, Source2Viewer and Blender, then run the setup checks.
2. **Character & model:** choose the hero to replace and your model file. Optionally choose a separate gun file. Preparation starts automatically.
3. **Prepare model:** usually finishes on its own; if it stops, it lists what to decide (a joint match, a shape-key choice) with a button to each spot.
4. **Appearance & weapons:** check the gun fit in the 3D preview, review the materials and textures, and accept them.
5. **Physics & preview:** switch movement on or off per part and preview the character.
6. **Build & install:** analyze, build and install; then check the character in Deadlock.

Projects are saved as `.dmb.json` files. The full walkthrough, with what each step does, is in [docs/MODEL_PREPARATION.md](docs/MODEL_PREPARATION.md).

## Documentation

| Document | Contents |
| --- | --- |
| [Model preparation](docs/MODEL_PREPARATION.md) | The five steps, scene isolation, bone mapping, textures, moving parts, gun customization, verification |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Compatibility limits and fixes for common failures |
| [Development](docs/DEVELOPMENT.md) | Build, test and Blender harness commands |
| [Releasing](docs/RELEASING.md) · [Release validation](docs/RELEASE_VALIDATION.md) | Release procedure and the checks a release needs |
| [Release notes](docs/RELEASE_NOTES.md) · [Changelog](CHANGELOG.md) | What changed, and what is still unverified in game |
| [Third-party notices](docs/THIRD_PARTY.md) | Licenses of bundled components |

## Building from source

Prerequisites on Windows:

- Node.js 24 with npm (`.node-version`)
- Rust 1.99.0, MSVC target (`rust-toolchain.toml`)
- Visual Studio Build Tools with *Desktop development with C++* and a Windows SDK
- Microsoft Edge WebView2 Runtime
- PowerShell 7 for the build and release scripts

```powershell
npm ci
node scripts/check-repository.mjs
npm test -- --maxWorkers=2
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked
npm run tauri -- dev
```

`npm run dev` serves only the browser frontend; native operations need `npm run tauri -- dev`.

The same tests, portable ZIP and startup check as CI:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

Outputs land in `artifacts/release/` (ZIP and checksums) and `artifacts/validation/` (logs). Add `-Release` to also build the installer. Nothing is published by this script; releases go through the manual **Release** workflow described in [docs/RELEASING.md](docs/RELEASING.md). The optional Blender harness that runs 28 awkward rigs through the whole preparation is described in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Project structure

| Path | Purpose |
| --- | --- |
| `src/` | React interface and its tests |
| `src-tauri/src/` | Tauri desktop host and commands |
| `src-tauri/crates/` | Rust pipeline: preparation, materials, skeleton/DMX export, cloth, compilation, VPK, storage |
| `src-tauri/crates/deadlock-preparation/resources/blender/` | Python scripts embedded into the app and run inside Blender |
| `resources/` | Hero catalog, tool recipe and hash-pinned native readers |
| `physics-references/` | Qualified movement references used by validation |
| `tools/cloth-reader/` | Source of the independent cloth reader |
| `scripts/` | Repository checks, Windows packaging, startup check and Blender test harnesses |
| `docs/` | Documentation |
| `.github/workflows/` | CI: tests and portable build on push; manual release workflow |
| `licenses/` | Third-party notices |

## Troubleshooting

- **A tool is "not configured":** choose the actual executable in Setup and run the checks again; a passing version check does not make every input file compatible.
- **Preparation stops:** the Prepare model step names the object or joint and offers a button to it. Rigs driven by live controls may need a baked export.
- **Build fails in the compiler:** keep the complete toolset together (do not mix compiler DLL versions) and copy the full log from **Logs**.
- **A validation blocks the build:** fix the reported input; bypassing a check produces a broken or unchanged mod.
- **Wrong model in game:** check the hero's model path, the VPK slot and conflicting mods.
- **Movement differs in game:** keep the prepared moving parts, choose the intended Physics mode, analyze and build again.

More in [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Keep changes focused, run the checks above, and in bug reports include the app version, the source file format, the hero and model, the steps and the relevant logs.

## License

The application source is licensed under the [MIT License](LICENSE). Bundled third-party components keep their own licenses; see [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md). The license grants no rights to Valve assets or to users' models and textures.

This is an unofficial tool, not affiliated with or endorsed by Valve. Deadlock and its assets belong to their respective owners.
