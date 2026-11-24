# Development

## Windows

Use Node 24, Rust 1.99.0, PowerShell 7, MSVC C++ Build Tools and a Windows SDK. Install WebView2 Runtime. `rust-toolchain.toml` selects Rust automatically; run `rustup target add x86_64-pc-windows-msvc` if needed.

```powershell
npm ci
node scripts/check-repository.mjs
npm test -- --maxWorkers=2
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked
npm run tauri -- dev
```

`npm run dev` serves only the browser frontend; native operations need `npm run tauri -- dev`.

The two lockfiles are required. Do not update dependencies while fixing a release unless the change requires it. A normal app build does not require Python; Blender supplies its own interpreter and the independent reader is self-contained. The .NET SDK is needed only for the effect player (see [Effect player](#effect-player)): a local build without it goes on without the player, a release build stops.

## Project structure

| Path | Purpose |
| --- | --- |
| `src/` | React interface and its tests (`src/theme/` is the Theme Manager and customizer) |
| `src-tauri/src/` | Tauri desktop host and commands |
| `src-tauri/crates/` | Rust pipeline: preparation, materials, skeleton/DMX export, cloth, compilation, VPK, storage, tools |
| `src-tauri/crates/deadlock-preparation/resources/blender/` | Python scripts embedded into the app and run inside Blender |
| `resources/` | Hero catalog, tool recipe and hash-pinned native readers |
| `physics-references/` | Records of the Vindicta cloth references; the compiled models are optional data |
| `tools/cloth-reader/` | Source of the independent cloth reader |
| `tools/effect-player/` | Source of the effect player (the moving preview on Extras → Effects) |
| `scripts/` | Repository checks, Windows packaging, the effect player's publish script, startup check, third-party notices and the Unihan readings generator |
| `site/` | The website (built by the Website workflow) |
| `docs/` | Documentation |
| `.github/workflows/` | CI: tests and portable build on every push; manual release workflow; the website |
| `licenses/` | Third-party notices |

## Distribution

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

The script installs locked npm dependencies, runs frontend and all default Rust workspace tests, builds the MSVC x64 Tauri application, stages portable resources from the Tauri configuration, verifies their hashes, launches the copied app and reader, and produces checksums. It stops on the first failure. It does not tag, push, or publish. To include an NSIS installer for an explicitly requested release, add `-Release`. Push/PR CI uses the portable-only default; manual workflow dispatch offers a release checkbox.

Outputs land in `artifacts/portable/` (the portable app), `artifacts/release/` (checksums; the installer with `-Release`) and `artifacts/validation/` (logs). Releases are published only through the manual **Release** workflow (`.github/workflows/release.yml`).

`-SkipStartupSmoke` is available only for local environments without an interactive Windows desktop. CI never uses it. A skipped startup check must not be described as verified.

## Other development hosts

The frontend tests/build and default non-desktop Rust workspace tests can run on Linux x64. The supplied Linux reader supports the independent native-format tests; it is excluded from Windows distributions. Native Windows launch and MSVC behavior still require Windows.

Native and game-file tests marked `#[ignore]` require separately supplied inputs and are not silently treated as passing. Read the ignore reason and the test's required environment variables before running one. Default fixture tests require no Steam login, proprietary compiler installation, Blender install, or live game.

## Byte-sensitive fixtures

`.gitattributes` keeps exact fixture and resource bytes. Do not enable automatic JSON formatting across fixture directories. Reader hashes are anchored both in `resources/native/cloth-reader/manifest.json` and Rust production trust pins. Rebuilding the reader is an intentional qualification change, not a routine CI step; see its [README](../tools/cloth-reader/README.md).

## Optional data

The TangTang presets, the two compiled Vindicta cloth references (`physics-references/*/…vmdl_c`) and some test data are optional. They are read from a `private-assets/` folder at the repository root (ignored by git), or from the folder `DMB_PRIVATE_ASSETS` names:

```powershell
cargo clean --manifest-path src-tauri/Cargo.toml -p deadlock-cloth-authoring   # after adding the folder to an existing checkout
node scripts/sync-private-assets.mjs   # copies the Vindicta references into physics-references/ (build-windows.ps1 does this itself)
```

Without the folder everything builds and tests. The Physics page then offers only Automatic movement and hides the included Vindicta reference, and a project saved with a TangTang preset fails Analyze with "not included in this build". Tests that need the data print a `skipped:` note on stderr and pass; run them with `-- --nocapture` to see the notes. The workflows (`windows.yml` and the Release workflow) build without the folder, so their builds and published releases never contain the optional data.

## Runtime resources

Tauri's `bundle.resources` is the source of truth for both installer and portable staging. The portable app must retain the folder layout next to its executable. `scripts/package-windows.mjs` rejects missing sources, wrong architecture, stale version names, or a mixed existing output folder. Production model and cloth algorithms are independent of these packaging scripts.

After changing Rust or npm dependencies, run `node scripts/generate-third-party-notices.mjs` (offline; needs `npm ci` and the crates in the local Cargo registry) to rewrite the shipped notices `licenses/THIRD-PARTY-RUST.txt` and `licenses/THIRD-PARTY-WEB.txt`; `scripts/check-repository.mjs` fails when they are missing, when the web list differs from `package-lock.json`, or when a listed crate version is no longer in `Cargo.lock` (a newly added crate is not detected).

### Effect player

`scripts/build-windows.ps1` publishes `tools/effect-player` with `scripts/publish-effect-player.ps1` before the app build. The script can also be run alone (`pwsh -NoProfile -File scripts/publish-effect-player.ps1`). It needs the .NET SDK named in `tools/effect-player/global.json` (11.0.100): set `DMB_DOTNET` to its `dotnet.exe`, or have it on PATH. The restore is locked (`packages.lock.json`, nuget.org only). The script checks the single file, its contents against `licenses/effect-player/DEPENDENCIES.json` and a few smoke runs, then stages `resources/native/effect-player/win-x64/Dmb.EffectPlayer.exe` (git-ignored).

Without the SDK a local build goes on without the player, and effect previews show the color strips only. `-Release` builds, and builds with `DMB_REQUIRE_EFFECT_PLAYER=true`, fail without it. The bundle entry is `src-tauri/tauri.effect-player.conf.json`, passed to `tauri build --config` only when the player was staged; pass it to `tauri dev` too to run the player in development. After changing the helper's packages, publish it and run `node scripts/generate-effect-player-notices.mjs`; `check-repository` fails when `licenses/effect-player/` no longer matches the lock file.

## End-to-end build of a project copy

`src-tauri/tests/e2e_build.rs` takes one saved project through the app's own pipeline (optionally Prepare again, then Analyze and Build) to check a toolset change, for example compiling models with Reduced CSDK 13 instead of the downloaded model toolset. It is ignored by default and reads only environment variables:

```powershell
$env:DMB_E2E_SETTINGS = "$env:APPDATA\com.deadlockmodbuilder.migration\settings.json"   # only read
$env:DMB_E2E_PROJECT  = 'C:\Path\To\Project.dmb.json'                                  # only read
$env:DMB_E2E_SCRATCH  = 'C:\Temp\e2e\run-1'                                            # new or empty
$env:DMB_E2E_OVERRIDES = '{"cswin":"","cswin_resourcecompiler":"","model_game":"","model_backend":"auto"}'  # optional
$env:DMB_E2E_PREPARE  = '1'   # optional: Prepare again first (needed after a game update changed the hero)
cargo test --manifest-path src-tauri/Cargo.toml --locked --test e2e_build -- --ignored --nocapture
```

The run copies the settings (with the overrides) and the project under a new project ID into the scratch folder, copies the project's prepared files into a new workspace there, and turns install off; the VPK stays in the scratch workspace. It refuses a scratch folder inside the user's workspace, project folder, Community SDK, model toolset, game or settings folder, checks that the game's addons folder is unchanged, and writes `e2e-report.json` (result, timings, log, and the new `dmb_<id>` compiler folders it created in the SDK and toolset, which it leaves for you to delete). Use a short scratch path: the compiler and Blender stop at long paths.

## Windows CI prerequisites

The workflow selects the Windows Server 2022 image, Node 24 and Rust 1.99.0. Before startup testing, it checks Microsoft's documented WebView2 registry locations and installs the Microsoft-signed Evergreen bootstrapper only if the runtime is missing. This prerequisite step is separate from ordinary local builds.

The runner's default `TEMP` is an 8.3 short path (`C:\Users\RUNNER~1\...`). The app resolves paths to their long spelling, and several Rust tests compare those with their temporary folder, so the workflows point `TEMP` and `TMP` at `runner.temp`. Locally, if your `TEMP` contains a `~1` component, set it to a folder without one before running the Rust tests.

References: [Tauri Windows build prerequisites](https://v2.tauri.app/start/prerequisites/), [Tauri Windows distribution](https://v2.tauri.app/distribute/windows-installer/), [Microsoft WebView2 runtime distribution](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution).

## Rest-skeleton recovery

The rest-skeleton planner (`source_compatibility.py` in the embedded Blender scripts) changes no Blender datablocks. `describe` emits the verified effective parents and role hints; native mapping, review and Blender conversion still validate them. Retarget attachment ancestry uses that same effective hierarchy with the original source matrices.

Biped digit aliases follow Autodesk's [MotionBuilder Biped template](https://help.autodesk.com/cloudhelp/2026/ENU/MOBU-PYTHON-API-REF/_tasks_0c3ds_max_biped_template_8py-example.html). Finger mapping still requires matching chain lengths and hand ancestry.
