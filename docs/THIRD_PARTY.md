# Dependencies and asset provenance

The application source uses the existing MIT license in the repository root. That license expressly excludes rights to Valve game assets, user models/textures, CSDK binaries, and external tools.

## Native reader

`resources/native/cloth-reader/` contains the qualified self-contained readers required by format-validation tests and the Windows app. The Windows binary is approximately 46 MB; the Linux development binary approximately 47 MB. Both are below GitHub's single-file hard limit and are deliberately versioned for exact trust-pin verification. The Linux binary is not packaged in Windows releases.

Reader source, SDK pin and NuGet lock are in `tools/cloth-reader/`. Component versions, source locations and license notices are in `licenses/cloth-reader/DEPENDENCIES.json` and adjacent files. The build does not silently fetch or replace the reader. Notices are included in both Windows distributions.

## Physics references and fixtures

The two compiled Vindicta references (`physics-references/*/…vmdl_c`), the TangTang preset data and the rig and cloth captures from third-party characters are not in this repository: they are private assets (see `docs/DEVELOPMENT.md`, Private assets). The repository keeps only their acceptance records and hash pins; `scripts/sync-private-assets.mjs` copies the compiled references in for a local build, and the tests that need a capture skip without it. The small format, animation, skeleton and model fixtures that remain under the Rust test directories are synthetic or reduced samples made to test the current implementation, not a game or content dump.

Game-derived data retains its original ownership. Its presence and user-provided provenance do not relicense it under MIT or establish redistribution permission; the application license cannot supply those rights. Do not add game dumps or third-party character models or captures to this repository; they belong in the private assets.

## Separately obtained tools

Deadlock, Source 2 compilation toolsets/CSDK, Blender, Source2Viewer CLI, and DepotDownloader are configured or obtained separately. Their ownership and terms remain with their respective providers. No CSDK installer, extracted full game dump, Steam credential, or local tool installation belongs in this repository.

## Unicode Unihan readings

`src-tauri/crates/deadlock-preparation/resources/blender/cjk_readings.txt` (about 150 KB, embedded in the app) lists the Mandarin reading of CJK ideographs, taken from the `kMandarin` field of the Unicode Unihan database (`Unihan_Readings.txt`, first reading, tone marks removed). It spells Chinese and Japanese material names in Latin letters. Copyright © 1991-2026 Unicode, Inc., under the Unicode License v3 in `licenses/Unicode-Unihan.txt`. `scripts/make_cjk_readings.py` rebuilds it from `Unihan.zip` (https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip).
