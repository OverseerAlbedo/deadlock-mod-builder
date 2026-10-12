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

The two lockfiles are required. Do not update dependencies while fixing a release unless the change requires it. A normal app build does not require Python or the .NET SDK; Blender supplies its own interpreter and the independent reader is self-contained.

## Distribution

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

The script installs locked npm dependencies, runs frontend and all default Rust workspace tests, builds the MSVC x64 Tauri application, stages portable resources from the Tauri configuration, verifies their hashes, launches the copied app and reader, and produces checksums. It stops on the first failure. It does not tag, push, or publish. To include an NSIS installer for an explicitly requested release, add `-Release`. Push/PR CI uses the portable-only default; manual workflow dispatch offers a release checkbox.

`-SkipStartupSmoke` is available only for local environments without an interactive Windows desktop. CI never uses it. A skipped startup check must not be described as verified.

## Other development hosts

The frontend tests/build and default non-desktop Rust workspace tests can run on Linux x64. The supplied Linux reader supports the independent native-format tests; it is excluded from Windows distributions. Native Windows launch and MSVC behavior still require Windows.

Captured native/game tests marked `#[ignore]` require separately supplied inputs and are not silently treated as passing. Read the ignore reason and the test's required environment variables before running one. Default fixture tests require no Steam login, proprietary compiler installation, Blender install, or live game.

## Byte-sensitive fixtures

`.gitattributes` keeps exact fixture and resource bytes. Do not enable automatic JSON formatting across fixture directories. Reader hashes are anchored both in `resources/native/cloth-reader/manifest.json` and Rust production trust pins. Rebuilding the reader is an intentional qualification change, not a routine CI step; see its [README](../tools/cloth-reader/README.md).

## Runtime resources

Tauri's `bundle.resources` is the source of truth for both installer and portable staging. The portable app must retain the folder layout next to its executable. `scripts/package-windows.mjs` rejects missing sources, wrong architecture, stale version names, or a mixed existing output folder. Production model and cloth algorithms are independent of these packaging scripts.

## Optional live-Blender probes

The `scripts/test_blender*.py` scripts exercise the active embedded bridge. Their small reference/fixture dependency set is in `legacy/python/`; it is development-only. Read the script before running it: some require a supplied character or target/options capture. For the generated pose-control fixture, a PowerShell example is:

```powershell
$env:DMB_PREP_TEST_OUTPUT = 'C:\DMB-tests\pose-controls'
& 'C:\Program Files\Blender Foundation\Blender 5.0\blender.exe' --background --factory-startup --python-exit-code 23 --python scripts/test_blender_pose_controls.py
```

Use your actual supported Blender path and a fresh output folder. These probes are outside default CI because Blender and some character inputs are separate requirements. They were preserved during repository cleanup, not rerun or newly certified by the release-only change.

## Windows CI prerequisites

The workflow selects the Windows Server 2022 image, Node 24 and Rust 1.99.0. Before startup testing, it checks Microsoft's documented WebView2 registry locations and installs the Microsoft-signed Evergreen bootstrapper only if the runtime is missing. This prerequisite step is separate from ordinary local builds.

The runner's default `TEMP` is an 8.3 short path (`C:\Users\RUNNER~1\...`). The app resolves paths to their long spelling, and several Rust tests compare those with their temporary folder, so the workflows point `TEMP` and `TMP` at `runner.temp`. Locally, if your `TEMP` contains a `~1` component, set it to a folder without one before running the Rust tests.

References: [Tauri Windows build prerequisites](https://v2.tauri.app/start/prerequisites/), [Tauri Windows distribution](https://v2.tauri.app/distribute/windows-installer/), [Microsoft WebView2 runtime distribution](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution).

## Rest-skeleton recovery regressions

`python scripts/test_source_compatibility.py` covers deterministic planning, healthy rigs, two-spine Biped layouts, unnamed intermediate joints, ambiguous pivots and conflicting parents. The planner changes no Blender datablocks. `describe` emits the verified effective parents and role hints; native mapping, review and Blender conversion still validate them. Retarget attachment ancestry uses that same effective hierarchy with the original source matrices.

For a private model, inspect with the Blender worker, run the Rust `mapping_report` example against a source/weighted/target JSON, then run `scripts/test_blender_rig_recovery.py` in Blender with `DMB_RECOVERY_SOURCE`, `DMB_RECOVERY_MAPPING` and `DMB_PREP_TEST_OUTPUT`. The script checks exact inspection preservation, save/reopen, both fit modes, preserved hair and correction re-export. Its positional Haze target and disposable generated material colors are explicit test fixtures, not live game/texture validation.

Biped digit aliases follow Autodesk's [MotionBuilder Biped template](https://help.autodesk.com/cloudhelp/2026/ENU/MOBU-PYTHON-API-REF/_tasks_0c3ds_max_biped_template_8py-example.html). Finger mapping still requires matching chain lengths and hand ancestry.

## Any-humanoid and gun probe

`scripts/test_blender_any_humanoid.py` builds awkward synthetic sources with `scripts/blender_fixtures.py` (generic, Rigify, DAZ, Character Creator and hand-named rigs; cluttered scenes; posed rigs with constraint-attached guns; Y-up exports; shape keys under subdivision; curve hair; collection instances; multiple scenes; FBX/GLB round trips; a gun-only weapon model; an unrigged statue) and runs each through the embedded worker, the native `e2e_probe` planner and the native DMX validator against the captured 196-bone hero in `tests/fixtures/daki-rig.json`. It does not use `legacy/python`.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml -p deadlock-preparation --example e2e_probe
& 'C:\Program Files\Blender Foundation\Blender 5.0\blender.exe' --background --factory-startup --python scripts/test_blender_any_humanoid.py -- C:\DMB-tests\any-humanoid
```

The gun reference used there is synthetic; production measures the decompiled original gun meshes. See [model preparation](MODEL_PREPARATION.md).
