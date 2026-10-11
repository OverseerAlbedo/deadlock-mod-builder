# Deadlock Mod Builder: notes for Claude Code

Tauri desktop app (Rust backend in `src-tauri/`, React/TypeScript UI in `src/`)
that turns a character file into a Deadlock hero replacement. Blender is used
headless as a geometry engine through Python scripts embedded in
`src-tauri/crates/deadlock-preparation/resources/blender/`. Everything else is Rust.

Read first: `docs/MODEL_PREPARATION.md` (preparation, scene objects, textures,
moving parts, mesh mapping, gun customization), `docs/DEVELOPMENT.md` (build and
test commands), `CHANGELOG.md`, `docs/RELEASE_VALIDATION.md`.

## Build and test (Windows, PowerShell)

```powershell
npm ci
node scripts/check-repository.mjs
npm test -- --maxWorkers=2
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked
pwsh -NoProfile -File scripts/build-windows.ps1     # portable app in artifacts\
```

Blender harness (18 awkward humanoid/gun cases, ends in native DMX validation):
build `-p deadlock-preparation --example e2e_probe`, then run
`scripts/test_blender_any_humanoid.py` with Blender (see DEVELOPMENT.md).
`src-tauri/target` grows to tens of GB; `cargo clean` is safe.

## State as of 0.13.0-alpha.51 (October 11, 2026)

Alpha 51 = Extras icon discovery (from a separate branch) merged with:
any-humanoid preparation and scene isolation, gun customization, Windows path and
snapshot fixes, texture recovery for game rips, moving-part detection (robes,
sashes, unnamed hanging chains), and the checkbox Mesh mapping with suggested
mapping and an unused-mesh warning. Rust workspace 476 passed / 27 ignored, UI
141 passed on Linux. Windows build and in-game checks are pending.

Test project: "Ghist" (Lady Geist, `geist.vmdl_c`), source
`CH10027_01_Rigged.blend` (modular rig: Body_Rig + Face_Rig, detached props Rose
and Arti10027, robe streamers `Bone_Robe_L/R_00-04`).

## Rules that are easy to break

- Moving-part detection exists three times and must stay identical:
  `deadlock-preparation/src/secondary_bones.rs`, `resources/blender/secondary_bones.py`,
  `src/preparation/secondary-bones.ts`. They share
  `deadlock-preparation/tests/fixtures/hanging-chains.json`; extend that fixture
  and all three tests when the rules change.
- Part kinds (hair, tail, streamer, cloth) are classified separately in
  `deadlock-cloth-authoring/src/parts.rs` (`semantic`, `analyze`).
- `.gitattributes` keeps fixture bytes exact; do not reformat fixture JSON.
- Do not update dependencies or lockfiles unless the change needs it.
- Version must agree in package.json, package-lock.json, Cargo.toml files and
  tauri.conf.json (`node scripts/check-repository.mjs` checks this).

## Planned work (agreed with the user)

1. **Storage.** Workspaces live in `%LOCALAPPDATA%\DeadlockModBuilder\workspaces\<project id>`
   and are never pruned (about 21 GB on the user's PC). Fix, in this order:
   - Prepare creates `humanoid_preparation/<uuid>` per run
     (`deadlock-preparation/src/service.rs`, around the "Stage character source"
     step); keep only the run the project uses (plus optionally the previous one).
   - `backups/base-*` (original hero sources, `deadlock-pipeline/src/sources.rs`)
     and replaced VPKs (`publish_copy` in `deadlock-pipeline/src/package.rs`)
     accumulate; keep the last one or two.
   - Workspaces of unsaved/discarded projects remain; offer to remove them.
   - Rebuildable build data (`split-dmx`, `native-model`, `build-evidence`,
     `material-cache`) can be cleared after a successful build.
   - Share the extracted original hero (`base`) per hero and game version
     instead of per project.
   - A **Storage** section in Settings (`src/components/SettingsPage.tsx`) that
     shows size per project and per kind of data, with Clear buttons and
     retention settings. The user specifically wants this.
2. **Breast jiggle (optional toggle, natural, not overdone).** Detect breast bones
   (names and geometry: paired chest-children in front of the chest) and give
   them a gentle spring profile through the existing secondary-motion path
   (`deadlock-cloth-authoring/src/secondary.rs` profiles,
   `deadlock-pipeline/src/motion.rs`). Off by default; tune in game.
3. **Hair false positives.** Halos and hats are sometimes detected as hair and
   get hair motion. Rigid head accessories (halo, hat, crown, horn, helmet, ear
   ornaments, glasses) must not become hair; check names and geometry (rigid,
   closed or ring-shaped, not a hanging chain) in the three detectors and in
   `parts.rs`.
4. **Experimental color editor.** Simple per-material hue/saturation/brightness
   or tint for models. Build on Appearance → Surface changes (tint already exists,
   `src/components/MaterialEditor.tsx`, `deadlock-materials`).

Also see the user's "Character Target Mapping Issue" document if they share it.
