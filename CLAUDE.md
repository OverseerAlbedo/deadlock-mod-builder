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

## Repository and release

Private GitHub repo `OverseerAlbedo/deadlock-mod-builder`, default branch `main`.
0.13.0-alpha.52 (tag `v0.13.0-alpha.52`) is a GitHub pre-release with the
installer .exe attached. Releases follow `docs/RELEASING.md`: bump all version
files together, update `CHANGELOG.md` and `docs/RELEASE_NOTES.md`, push, then run
the manual **Release** workflow (`.github/workflows/release.yml`), which tests,
builds the installer, tags the commit and publishes the pre-release. The push
workflow (`.github/workflows/windows.yml`) only builds and tests; it never publishes.

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
- Part kinds (hair, tail, streamer, cloth, breast, rigid) are classified separately in
  `deadlock-cloth-authoring/src/parts.rs` (`semantic`, `analyze`). Its names reuse
  `named_breast` and `rigid_named` from `secondary_bones.rs`.
- `scripts/test_blender_moving_parts.py` checks the Python detector against the
  shared fixture (run it with Blender).
- Gun planning is `weapon::plan_choice` / `suggest_choice` / `preview_choice`
  (the service and `examples/e2e_probe.rs` both use them). Which guns are left out
  and which hand carries a charm exists three times and must stay identical:
  Rust `secondary_bones::selection_for` (`Plan::hands`, `Plan::removed_ids`),
  Python `secondary_bones.selection` (`gun_hands`, `removed_guns`) and TS
  `removedGuns` in `src/preparation/secondary-bones.ts`. The left/right name rules
  exist twice: `weapon.rs side_of` and `weapon_source.py side_of`.
- Gun preview shapes are simplified twice with the same vertex clustering and
  rounding: `weapon.rs simplify` (hero guns and body) and `weapon_fit.py _shape`
  (candidate guns). Part-name words for a gun's front, rear and grip are in
  `weapon_fit.py` (`_hints`); `weapon.rs fit` uses them.
- Blender export loops go through `resources/blender/mesh_export.py` (bulk
  readers, cached vertex-group rows); do not add per-corner Python loops to
  `worker.py`/`weapon_fit.py`. Receipts hold `PreparedMesh::slim` meshes (no
  geometry) after Convert; read geometry from the DMX, never from the receipt.
  Profile a conversion with cProfile around `runpy.run_path(worker.py)`.
- Prepared material names keep `NAME_CHARS` (24) readable characters
  (`material_textures.py`); `deadlock-materials native_paths.rs`
  `PREPARED_NAME_CHARS` must match. The 8-digit hash is the material's identity
  (`service.rs renamed_materials` carries choices over by it).
- `.gitattributes` keeps fixture bytes exact; do not reformat fixture JSON.
- Do not update dependencies or lockfiles unless the change needs it.
- Version must agree in package.json, package-lock.json, Cargo.toml files and
  tauri.conf.json (`node scripts/check-repository.mjs` checks this).

## Implemented after Alpha 51 (uncommitted on master; needs Windows and in-game checks)

The four planned items and the Rem target issue are implemented; see the
`Unreleased` section of CHANGELOG.md.

- **Storage**: `deadlock-pipeline/src/storage.rs` (classification, report, clear,
  retention; never removes paths a project references, locked workspaces, or
  compiler folders whose `.deadlock-builder.json` names another project) and
  `src-tauri/src/storage.rs` (commands `storage_report`, `storage_clear`,
  `storage_find_projects`; registry `known-projects.json` in the app config dir;
  pruning after Inspect/Convert/Accept). UI: `src/components/StorageSection.tsx`.
  Settings fields `keep_backups`, `keep_previous_preparation`,
  `clear_build_data_after_build`. Game-archive original hero sources are shared in
  `<workspace>/shared-originals/<key>` (`sources::base_dir`); imported folders
  stay per project.
- **Breast motion**: `breast_motion` preparation option (unset = old behavior),
  `breast_roots` in the three detectors, `physics_kinds` option read by
  `secondary::setup`; `append_breast` and `link_breast_pairs` in `secondary_auto.rs`
  build the breast proxy (ring behind each breast, linked left/right pair).
  Rigs without breast bones: `resources/blender/breast_bones.py` adds
  `DMB_Breast_L/R` at Inspect when `breast_motion` is on
  (`secondary_bones::breast_request`, `Analysis.breast_generation`, repeated at
  Convert). Tuning was loosened after the first in-game test barely moved
  (now goal 0.45, drag 0.1, travel ~45% of breast radius).
- **Rigid accessories**: `RIGID_HINTS`, `rigid_named`, `ring_chain` in the three
  detectors; `Kind::Rigid` and the ring rule in `parts.rs`.
- **Color editor**: `hue`/`saturation`/`brightness` on `AppearanceMaterial`,
  `deadlock-materials/src/color_adjust.rs` (adjusted copy of the staged
  base-color image), UI in `MaterialEditor.tsx` (the CSS preview is approximate).
- **Game targets (Rem)**: `heroes::GameModels` reads `scripts/heroes.vdata`;
  `get_heroes` returns `hero` and `game` per model; Analyze warns on a target the
  game does not load; the roster's Rem main entry is `familiar_wip.vmdl`.

Still to do: Windows build (`scripts/build-windows.ps1`), in-game checks of
breast motion amounts, rigid accessories, color edits and a Rem build, and a real
Storage cleanup on the user's PC (dry run: 37 GB total, 28 GB clearable).

