# Releases

A release requires an explicit user request. Normal development never increments a public version, creates a tag, or publishes a release. The Windows workflow has read-only repository permission and uploads temporary CI artifacts only.

## Current handoff

Version: **0.13.0-alpha.46**. Intended tag: **v0.13.0-alpha.46**. This preserves the accepted Alpha 46 application behavior. The new release work changes repository organization, CI, documentation, and packaging of notices.

No repository destination was supplied with this handoff. No remote was pushed, no tag was created, and no GitHub release or hosted Windows check is claimed. See [validation record](RELEASE_VALIDATION.md).

## GitHub Desktop: initial upload

1. Create the intended repository in GitHub Desktop. Copy the **contents** of the supplied source folder into its repository root. `README.md`, `package.json`, `src/`, `src-tauri/`, and `.github/` must be directly at that root.
2. Do not copy `release-assets/`, installers, portable ZIPs, generated `artifacts/`, or the outer handoff ZIP into Git history. The supplied `.gitignore` covers generated directories.
3. Review Changes and the intended repository/branch. Suggested summary: `chore: prepare Deadlock Mod Builder GitHub release`.
4. Commit and publish/push to the intended repository. Repository visibility and permissions are the owner's choice.
5. In GitHub Actions, open **Windows tests and portable application** and wait for its **windows** job on that exact commit. Inspect the first failing step and validation artifact if it fails. Do not call the hosted check passed based on local cross-compilation.

## Requested installer build

After the portable push check passes, run the same workflow manually for the exact intended commit with its **release** checkbox enabled to also produce the installer, or run `pwsh -NoProfile -File scripts/build-windows.ps1 -Release` on Windows. This is an explicit one-time build action and still does not publish.

## One-time publication

After the requested version/destination is confirmed and the exact commit passes:

1. Select/create tag `v0.13.0-alpha.46` at that tested commit. If it already exists elsewhere, stop and resolve the version; do not overwrite a published tag.
2. Draft a release titled `Deadlock Mod Builder 0.13.0-alpha.46` and mark it **Pre-release**.
3. Use the manual release-build CI artifact's installer, portable ZIP, and `SHA256SUMS.txt` for the tested commit. Prepared cross-built assets are also included with this handoff, but do not imply they were executed on Windows here.
4. Paste [release notes](RELEASE_NOTES.md), attach the files individually, and publish only this requested release.
5. Check the tag target, prerelease flag, asset names, downloadable files, and checksums.

Future releases require a new explicit request. There is no automatic release workflow.

## Version consistency

Change versions together in `package.json`, root entries of `package-lock.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/crates/deadlock-pipeline/Cargo.toml`, and the corresponding packages in `src-tauri/Cargo.lock`. Run repository preflight before building. No unrelated dependency upgrades belong in a version bump.
