# Dependencies and asset provenance

The application source is licensed under the GNU Affero General Public License v3.0 or later (`LICENSE` in the repository root; the copyright notice and the scope of the license are in `NOTICE`). Versions up to and including 0.13.0 were published under the MIT License. The license expressly excludes rights to Valve game assets, user models/textures, CSDK binaries, and external tools.

## Rust crates and web packages

The Rust crates compiled into the Windows app and the production npm packages bundled into its interface are listed, with their versions, licenses, source repositories and the license and notice files they carry, in `licenses/THIRD-PARTY-RUST.txt` and `licenses/THIRD-PARTY-WEB.txt`. Both files ship with the app. `scripts/generate-third-party-notices.mjs` writes them from the lockfiles; run it again after dependency changes (see [Development](DEVELOPMENT.md)). `licenses/roxmltree.txt` is the MIT notice of `roxmltree`, the XML parser that checks the game's vector icons before they are previewed (`deadlock-components/src/vector_icons.rs`).

## MPL-2.0 crates

`cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors` and the `symphonia` crates are licensed under the Mozilla Public License 2.0. They are used unmodified; their source is available from crates.io at the versions pinned in `src-tauri/Cargo.lock` (listed in `licenses/THIRD-PARTY-RUST.txt`, which also holds the MPL-2.0 text).

## Microsoft WebView2 loader

The Windows app links Microsoft's WebView2 loader statically (`WebView2LoaderStatic.lib`, through the `webview2-com-sys` crate) to start the Microsoft Edge WebView2 Runtime, which Windows provides or which is installed separately. The loader is Microsoft's, published with the WebView2 SDK (`Microsoft.Web.WebView2`) under Microsoft's BSD-style license, which allows redistribution in binary form with its copyright notice. That notice ships with the app as `licenses/Microsoft-WebView2.txt`.

## Application icons

The application icons (`src-tauri/icons/`) were made by the project's author.

## Source 2 Viewer

The app is built on [Source 2 Viewer](https://s2v.app) ([ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat), MIT License): its command-line tool, installed separately, extracts and decompiles game assets, and its library is part of the native reader below.

The app's own reader and writer of compiled animation clips (`deadlock-preparation/src/clip_decode.rs`, `clip_encode.rs`) follow the clip data layout of ValveResourceFormat's public decoder (`AnimationClip.cs`, MIT License); no code was copied.

## DMX reader and writer

The app's DMX reader and writer (`src-tauri/crates/deadlock-dmx`) are a new implementation of a subset of the binary format (versions 4, 5 and 9), written with reference to the binary type layout and string and reference conventions of Blender Source Tools' `datamodel.py` by Tom Edwards (MIT License). That notice ships with the app as `licenses/BlenderSourceTools-DMX.txt`.

## Native reader

`resources/native/cloth-reader/` contains the qualified self-contained readers required by format-validation tests and the Windows app. The Windows binary is approximately 46 MB; the Linux development binary approximately 47 MB. Both are below GitHub's single-file hard limit and are deliberately versioned for exact trust-pin verification. The Linux binary is not packaged in Windows releases.

Reader source, SDK pin and NuGet lock are in `tools/cloth-reader/`. Component versions, source locations and license notices are in `licenses/cloth-reader/DEPENDENCIES.json` and adjacent files. The build does not silently fetch or replace the reader. Notices are included in both Windows distributions.

## Effect player

The effect preview uses a helper program, `native/effect-player/win-x64/Dmb.EffectPlayer.exe`, built from `tools/effect-player` on ValveResourceFormat.Renderer 21.1.7798 and OpenTK 4.9.4. It is one self-contained file that includes the .NET 11 runtime. The notices of everything inside it are in `licenses/effect-player/`; `DEPENDENCIES.json` there lists each package with its version, license, source and the files it accounts for. Licenses: MIT (ValveResourceFormat, ValveKeyValue, ValvePak, KeyValues2, OpenTK, SkiaSharp, SharpGLTF.Core, TinyBCSharp, NLayer, K4os.Compression.LZ4, Vortice.SPIRV and Vortice.SpirvCross, System.IO.Hashing, System.Numerics.Tensors, the .NET runtime), zlib (GLFW), Apache-2.0 OR MIT (SPIRV-Cross, used under Apache-2.0), and, for parts built into ValveResourceFormat, MIT, Apache-2.0, BSD-3-Clause and the SIL Open Font License 1.1 (the JetBrains Mono and Manrope fonts). Skia and its dependencies are covered by SkiaSharp's own notices.

## Physics references and fixtures

The two compiled Vindicta references (`physics-references/*/…vmdl_c`) are optional data: the repository keeps their records and hash pins, and `scripts/sync-private-assets.mjs` copies the compiled files in for a local build. The small format, animation, skeleton and model fixtures under the Rust test directories are synthetic or reduced samples made to test the current implementation, not a game or content dump.

Game-derived data retains its original ownership. Its presence and user-provided provenance do not relicense it under the application's license or establish redistribution permission; the application license cannot supply those rights. Do not add game dumps or third-party character models to this repository.

## Separately obtained tools

Deadlock, Source 2 compilation toolsets/CSDK, Blender, Source2Viewer CLI, and DepotDownloader are configured or obtained separately. Their ownership and terms remain with their respective providers. No CSDK installer, extracted full game dump, Steam credential, or local tool installation belongs in this repository.

## Unicode Unihan readings

`src-tauri/crates/deadlock-preparation/resources/blender/cjk_readings.txt` (about 150 KB, embedded in the app) lists the Mandarin reading of CJK ideographs, taken from the `kMandarin` field of the Unicode Unihan database (`Unihan_Readings.txt`, first reading, tone marks removed). It spells Chinese and Japanese material names in Latin letters. Copyright © 1991-2026 Unicode, Inc., under the Unicode License v3 in `licenses/Unicode-Unihan.txt`. `scripts/make_cjk_readings.py` rebuilds it from `Unihan.zip` (https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip).
