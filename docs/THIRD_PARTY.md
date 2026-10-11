# Dependencies and asset provenance

The application source uses the existing MIT license in the repository root. That license expressly excludes rights to Valve game assets, user models/textures, CSDK binaries, and external tools.

## Native reader

`resources/native/cloth-reader/` contains the qualified self-contained readers required by format-validation tests and the Windows app. The Windows binary is approximately 46 MB; the Linux development binary approximately 47 MB. Both are below GitHub's single-file hard limit and are deliberately versioned for exact trust-pin verification. The Linux binary is not packaged in Windows releases.

Reader source, SDK pin and NuGet lock are in `tools/cloth-reader/`. Component versions, source locations and license notices are in `licenses/cloth-reader/DEPENDENCIES.json` and adjacent files. The build does not silently fetch or replace the reader. Notices are included in both Windows distributions.

## Physics references and fixtures

The two compiled Vindicta references are required by the included-reference workflow and strict regression tests. The balanced resource's existing SHA-256 remains unchanged. Small format, animation, skeleton and model fixtures under Rust test directories are intentionally retained to test the current implementation. These are curated resources, not a complete game/content dump.

These game-derived assets retain their original ownership. Their presence and user-provided provenance do not relicense them under MIT or establish redistribution permission. Before publishing a public repository or release, the maintainer must resolve rights to retained external assets; the application license cannot supply those rights. Do not add unrelated game dumps or third-party character models.

## Separately obtained tools

Deadlock, Source 2 compilation toolsets/CSDK, Blender, Source2Viewer CLI, and DepotDownloader are configured or obtained separately. Their ownership and terms remain with their respective providers. No CSDK installer, extracted full game dump, Steam credential, or local tool installation belongs in this repository.
