# Releases

A release requires an explicit user request. Normal development never increments a public version, creates a tag, or publishes a release. Only the manually started Release workflow can publish.

## How releases work

- **Every push** runs **Windows tests and portable application** (`.github/workflows/windows.yml`): tests, build and startup check, with temporary artifacts only. It has read-only permission and never publishes.
- **A release** runs only when started by hand: **Actions → Release → Run workflow** (`.github/workflows/release.yml`), or `gh workflow run release.yml -f version=X.Y.Z-alpha.N`. On `main` only, it checks that the version matches `package.json`, that `CHANGELOG.md` has a section for it and that the tag and release do not exist yet. Then it runs the full build with the installer (`scripts/build-windows.ps1 -Release`: all tests and the portable startup check), creates the tag `vX.Y.Z-alpha.N` on that commit and publishes a GitHub pre-release titled `Deadlock Mod Builder X.Y.Z-alpha.N` with the installer `.exe` attached. The notes are `docs/RELEASE_NOTES.md` plus the installer's SHA-256. If any step fails, nothing is published. Published releases and tags are never replaced.
- **Private assets.** The TangTang presets are built from data outside the repository (see DEVELOPMENT.md "Private assets"). Both workflows check the private repository out when the `PRIVATE_ASSETS_TOKEN` secret holds a token that can read `OverseerAlbedo/deadlock-mod-builder-private-assets`; without it the installer carries no TangTang presets. The published notes end with "TangTang presets: included" or "not included", so check the secret before a release that players with TangTang conversions will use.

## Preparing a release

1. Change the version together in all version files (see below) and run `node scripts/check-repository.mjs`.
2. Turn the changelog's unreleased section into `## X.Y.Z-alpha.N — date`, and write `docs/RELEASE_NOTES.md` for players.
3. Commit and push to `main`, and let the push check pass.
4. Run the **Release** workflow with that version. A run takes about 30 minutes of Windows build time, which counts double against the free Actions minutes of a private repository.

## Current release

Version **0.13.0-beta.3** (tag `v0.13.0-beta.3`) in the private `OverseerAlbedo/deadlock-mod-builder` repository, default branch `main`; the fourth release made by the Release workflow, after 0.13.0-beta.2, 0.13.0-beta.1 and 0.13.0-alpha.52. Earlier handoffs (Alpha 46) were prepared without a repository destination. See [validation record](RELEASE_VALIDATION.md).

## Version consistency

Change versions together in `package.json`, root entries of `package-lock.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/crates/deadlock-pipeline/Cargo.toml`, and the corresponding packages in `src-tauri/Cargo.lock`. Run repository preflight before building. No unrelated dependency upgrades belong in a version bump.
