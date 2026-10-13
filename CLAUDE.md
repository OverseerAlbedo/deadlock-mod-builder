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
0.13.0-beta.1 (tag `v0.13.0-beta.1`, October 13, 2026) is the latest GitHub
pre-release, with the installer .exe attached; alpha.52 preceded it. The
repository allows squash and rebase merges only (no merge commits). Releases follow `docs/RELEASING.md`: bump all version
files together, update `CHANGELOG.md` and `docs/RELEASE_NOTES.md`, push, then run
the manual **Release** workflow (`.github/workflows/release.yml`), which tests,
builds the installer, tags the commit and publishes the pre-release. The push
workflow (`.github/workflows/windows.yml`) only builds and tests; it never publishes.

## State as of 0.13.0-beta.1 (October 13, 2026)

Beta 1 = alpha 52 (storage, breast motion, rigid accessories, color editor, Rem
target) plus: the five-step flow with automatic preparation, guns from a
separate file, dual-wield heroes, gun charms, the solid fit preview, textured
gun fixes (UV map order, tint, shared materials), finger curl, short material
names with a compiler path check, live VPK slot state, restyled setup pages,
and the performance pass (bulk mesh export, slim receipts, fewer file reads).
Rust workspace 493 passed / 27 ignored, UI 178 passed, Blender harness 28/28.

Test projects on the user's PC (`C:\Users\edome\OneDrive\Documents\DML_Projects`):
"YvvoneHaze" (Yvonne11.blend on Haze with the Artzy Tyrannical dual gun file
from `Artzy_Tyrannical_Textures_and_Models.zip`), "Ghist" (Lady Geist,
`CH10027_01_Rigged.blend`), "janeVen", "paimonRem".

Not yet checked in game: the finger bend around the gun, the charm swing
amount, dual-gun aim during animations, breast motion amounts, rigid
accessories, color edits and a Rem build.

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
