# Alpha 51 combined build validation (preparation, gun customization, icons)

Prepared on October 11, 2026 for **0.13.0-alpha.51**, after merging the Extras
icon discovery work (below) with the model preparation and gun customization
branch.

| Check | Result |
| --- | --- |
| Rust workspace | 476 tests passed; 27 existing ignored tests (includes the 40 icon/component tests) |
| Frontend | 141 tests passed across 28 files; TypeScript passed |
| Repository preflight | Version agreement, required resources and native-reader trust pins passed |
| Blender harness | 18 generated humanoid, scene, gun and weapon-model cases prepared and exported (`scripts/test_blender_any_humanoid.py`) |
| Real game-rip model | Modular rig with separate face rig and two detached props: 3 materials received base-colour textures, 7 moving-part groups kept (5 hair, 2 robe streamers), native DMX export and skin checks passed for both meshes |
| Moving-part rules | Rust, Blender and UI detection agree on a shared fixture |

The Alpha 51 icon-only row "456 preparation files match Alpha 50 byte for
byte" no longer applies: this build intentionally changes preparation. The
icon-only installer evidence in `docs/evidence/windows-package-alpha51.json`
describes the earlier icon-only package, not this combined build. Windows and
Deadlock were not executed here.

---

# Alpha 51 Extras icon discovery validation

Prepared on October 11, 2026 for **0.13.0-alpha.51**.

| Check | Result |
| --- | --- |
| Frontend | 128 tests passed across 25 files; TypeScript and Vite production build passed |
| Component regressions | 40 tests passed; 1 existing ignored test |
| Complete roster inventory | All 44 heroes and 3 alternate models returned images; zero available explicit image references omitted |
| Abrams / Lady Geist | 18 textures + 3 vectors / 19 textures + 5 vectors in the audited game snapshot |
| Real resource formats | ValveResourceFormat compiled VSVG and binary VData fixtures decoded successfully |
| Existing preparation and physics | 456 preparation, skeleton, cloth, reference and related UI/test files match Alpha 50 byte for byte |
| Repository preflight | Version agreement, required resources and native-reader trust pins passed |
| Windows x64 | Final application cross-built with the production frontend and standard release settings |
| Installer | NSIS archive CRC passed; 79 payload files verified, including 78 resource hashes and all 17 embedded Blender scripts |
| Packaged icon fix | Current alias catalog and the new discovery/vector-preview code verified inside the installed application |

The old filename-only scan missed legacy image names such as `bull` and
`spectre`. Discovery now follows the installed hero's explicit image references
and reachable ability records, and keeps name matching for alternate artwork.
Six legacy alias families are included. The existing texture decoder and
replacement implementation remain in place. Compiled vector logos are listed
with read-only previews, and newly reachable shared images require explicit
replacement consent during staging and receipt verification.

The inventory audit used the complete decompiled definitions and archive index
from GameTracking-Deadlock commit `e6bb7e33d212b110d138f98427ef37f9a80c2839`.
The three source blobs were checked against that repository tree, and their
SHA-256 values are recorded in `docs/evidence/extras-icon-inventory.json`.
The independently extracted compact inventory is committed as a regression
fixture. See `docs/EXTRAS_ICON_DISCOVERY.md` for all 47 per-model counts and
reproduction commands.

Three portrait references are absent from the game inventory itself: Deadman
Danny's `deadpack_card`, Nurse Harrow's `nurse_card`, and Violet's `artist_card`.
They produce a notice instead of an invented replacement destination. Their
other images remain available. The inventory audit verifies discovery and
filenames, not every image payload or rendered pixel. Windows and Deadlock
were not executed here; use **Extras → Reload assets** after installing.

Installer evidence is in `docs/evidence/windows-package-alpha51.json`. The
installed application differs from the portable binary only by Tauri's expected
three-byte NSIS bundle marker. The new XML parser's MIT notice is included in
`licenses/roxmltree.txt`. The successful build used a separate output directory
after incomplete object files in the original build cache caused linker errors;
application code, release optimizations and resource verification were retained.

---

# Alpha 50 humanoid autodetection and rest-rig recovery validation

Prepared on October 11, 2026 for **0.13.0-alpha.50**.

| Check | Result |
| --- | --- |
| Frontend | 127 tests passed across 25 files; TypeScript and Vite production build passed |
| Rust workspace | 453 tests passed; 27 existing ignored tests |
| Pure recovery planner | 6 tests passed, including ambiguous pivots, conflicting parents, chain recovery, aliases and healthy-rig preservation |
| Uploaded model, native Blender 5.0.0 | All 17 required body roles identified, compared with 10 before this change |
| Read-only recovery | 14 effective hierarchy links and 5 surviving helper roles; original bone matrices, parents, weights, mesh data and UVs unchanged |
| Saved intake / live controls | Saved intake reimport passed; a live pose constraint prevents disconnected-root recovery |
| Both fitting modes | Converted 24,238 vertices and 125 weighted bones; 18 hair bones retained in 5 groups; exact hero binds and correction handoff passed |
| Previous modular import | 13 native Blender cases passed, including the uploaded model |
| Previous selection / rest controls | 5 rig-selection and 7 pose-control cases passed |
| Previous secondary motion | 8 native Blender cases passed, including non-unit parent scales |
| Existing physics implementation | 361 cloth, physics-reference and skeleton files match Alpha 49 byte for byte |
| Repository preflight | Version agreement, required resources and native reader trust pins passed |
| Windows x64 | Final application cross-built successfully with the production frontend |
| Installer | NSIS archive CRC passed; 78 payload files verified, including all 17 embedded Blender scripts |

The missing follow buttons were a downstream symptom of incomplete body-role
recognition. This file has detached limb roots and surviving twist helpers in
place of several primary joints. A complete, unambiguous anatomical solution is
now required before using those helpers and inferring missing parent links.
The links exist only in the converter's effective hierarchy; Blender's imported
rest matrices and bone parenting are never rewritten by this recovery step.
Existing healthy rigs use their established path. Live controls, conflicting
parents and ambiguous candidates do not receive a guessed repair.

Broader recognition covers common Biped spine and finger names, Mixamo finger
names, ValveBiped/DEF/VRM name wrappers, and uniquely anchored unnamed chain
joints. Follow buttons remain visible with zero suggestions, and the page
explains incomplete body roles. Refreshing suggestions can retain manual role
choices; stale suggestions cannot be applied after a role change.

The real-model conversion test uses the positional Haze regression fixture,
not a current compiled hero. It found 3 severe edge-strain observations with
`match_target` and 15 with `preserve_proportions`; these remain visible review
diagnostics, not a claim of artistic or in-game acceptance. Followed corrective
bones retain their offsets but do not reproduce independent source drivers.
Hair rigging is retained; this does not automatically create Source 2 simulation.

The uploaded model's external `Textures` folder was not supplied. Temporary
generated colors were used only in the disposable test scene. The source file's
SHA-256 is unchanged, and the private model is not included in the source archive.
Keep original textures beside the model when testing locally.

See `scripts/test_source_compatibility.py`, `scripts/test_blender_rig_recovery.py`
and `docs/evidence/rig-recovery.json` for reproducible checks and their scope.
Installer evidence is in `docs/evidence/windows-package-alpha50.json`: all 77
resource hashes match, and the installed application differs from the portable
build only by Tauri's expected three-byte NSIS bundle-type marker.
Windows, the native game compiler and Deadlock were not executed here.

---

# Alpha 49 modular character import validation

Prepared on October 10, 2026 for **0.13.0-alpha.49**.

| Check | Result |
| --- | --- |
| Native Blender 5.0.0 | 13 modular rig cases passed, including the uploaded model |
| Native Blender 5.2.0 LTS | The same 13 cases passed |
| Previous rig selection | 5 cases passed in Blender 5.0 and 5.2 |
| Rest pose controls | 7 cases passed in Blender 5.0 |
| Rust preparation suite | 36 passed, 4 existing ignored tests |
| Embedded Blender scripts | Completeness test passed; all 16 scripts also verified inside the packaged EXE |
| Geometry and weights | Retained points, faces, UVs, material slots, weights and shape keys preserved |
| Face movement | Evaluated head rotation retains the original face offset and moves the full face together |
| Saved workspace | Staged synthetic fixtures reopen with the selection notice and complete actual preparation; uploaded model saves/reopens without geometry changes |
| Source preservation | Original source SHA-256 unchanged after tests |
| Production frontend / preflight | TypeScript, Vite, versions, resource layout and reader trust pins passed |
| Windows x64 | Application cross-built successfully |
| Installer | NSIS archive CRC passed; all 78 installed payload files match the package |
| Accepted UI, components and cloth | 268 frontend, component, cloth and physics-reference files match Alpha 48 byte for byte |

The uploaded model retains Body and Face (24,238 vertices, 125 combined bones),
selects Body_Rig, preserves Face_Rig's offset head as a distinct attached bone,
and excludes the detached Arti10027 and Rose props. Character selection uses
weighted anatomical coverage, with a humanoid geometry fallback. It does not
use this model's names or choose the rig with the most bones. Regression inputs
include renamed rigs and equipment with more bones than the character.

Shared bones are reused only when their world rest matrices agree. Distinct
part bones keep their transforms and weights. A local float32 copy allowance
of up to four millionths on orientation elements covers EditBone head/tail
rounding for tiny face bones; translation, scale/shear, skeleton equivalence
and compiled-bind compatibility checks are not relaxed.

Multiple clear characters, nearby ambiguous parts, incompatible relative rig
scale, unsupported object/rest controls and independent physics bindings on a
part rig remain explicit errors. Existing single-rig physics is unchanged.

The uploaded `.blend` references an external `Textures` folder that was not
provided. Import, head movement and save/reopen were checked on the real file;
full texture staging and hero-specific conversion were not. Synthetic fixtures
exercise staging, preparation and corrected-source reimport. Keep original
texture files alongside the source when importing on the user's machine.

Windows, Deadlock and hosted GitHub CI were not executed in this environment.
The cross-build and archive verification do not establish in-game acceptance.
The source fixture is not distributed in this archive. See
`scripts/test_blender_modular_rigs.py` and the compact native test evidence at
`docs/evidence/modular-character-import.json` to reproduce the checks.

---

# Alpha 48 drag-and-drop validation

Prepared on October 10, 2026 for **0.13.0-alpha.48**.

| Check | Result |
| --- | --- |
| Frontend tests | 125 passed across 25 files |
| Native drag regression | Reproduced the tab switch before the fix; entry outside the target, movement, cancellation and drop into the icon box pass after the fix |
| Windows display scaling | 200% coordinates resolve the child element back to its replacement box |
| Saved icon replacement | Dropped image path and selected icon target reach the saved project; model parser is not called |
| Model import | Dragging preserves the current page; an accepted model drop still opens Appearance and runs analysis |
| Production frontend | TypeScript and Vite build passed |
| Repository preflight | Versions, required resources and native reader trust pins match |
| Windows x64 | Application cross-built successfully |
| Installer | NSIS archive CRC passed; all 78 installed payload files match the package |
| Accepted backend and physics | 414 component, preparation, cloth and physics-reference files match Alpha 47 byte for byte |

This is a frontend drag-routing fix. The global window handler no longer navigates
when a file enters the window, so it cannot unmount the intended replacement box
before the cursor reaches it. Existing target validation and model-drop handling
remain in place. The tests exercise the native event callback; Windows and
Deadlock were not executed in this environment. Hosted Windows CI was not run.

The previous icon replacement checks are retained below as the Alpha 47 record.

---

# Alpha 47 icon replacement validation

Prepared on October 10, 2026 for **0.13.0-alpha.47**.

| Check | Result |
| --- | --- |
| Frontend tests | 124 passed across 25 files |
| Rust workspace regression command | Passed (exit code 0) |
| Focused component suite | 34 passed, 1 existing ignored test |
| Real texture corpus | 6 Source 2 texture samples replaced, packed into VPKs, and decoded by Source2Viewer CLI 20.0 |
| Replacement color and transparency | Independently decoded center RGBA equals the source color (40, 80, 120, 90) for all six samples |
| Texture layout regressions | Fallback records, processing flags, padding, custom mip counts, compressed mips, and embedded PNGs covered |
| Tampering / malformed input | Corrupt pixels, invalid dimensions/layouts, stale originals and changed output remain rejected |
| Accepted cloth and model preparation | Source and required physics references unchanged |
| Repository preflight | Versions, resource layout and native reader trust pins match |
| Windows x64 | Application cross-built; five component test binaries cross-compiled |
| Installer archive | NSIS package archive CRC verified; all 78 payload files match, including the WebView2 loader, native reader, references and embedded app code |

The resizing regression now checks RGB color as well as opacity; the previous
byte-scale values supplied to a normalized floating-point resampler were the
cause of darkened previews and fitted replacements.

The six optional upstream samples cover DXT5, BC7, BGRA8888, RGBA8888, I8 and
embedded PNG storage. They are development inputs outside the source ZIP. The
portable regression fixtures are synthetic, including realistic fallback and
128-byte metadata records. Set `DMB_ICON_TEXTURE_CORPUS` to a directory of these
samples to repeat the additional corpus test.

## Scope and remaining limits

Ordinary 2D image handling is shared across heroes; there is no Graves-only rule.
New GPU replacements use RGBA storage, preserve the original dimensions, display
rectangle, mip count, clamp/LOD flags and runtime metadata, and replace stale
pixel-derived data. This can use more texture memory than the original compressed
image. Embedded PNG replacement remains PNG-based.

Arrays, cubes, volumes, sprite sheets, virtual textures, unknown encodings and
malformed files remain restricted. Sound and particle compatibility checks were
not bypassed. The screenshot alone is not the original Graves icon resource, so
its exact files still need an in-game check.

This environment did not execute Windows, the native game compiler, or Deadlock.
A Windows x64 cross-build and independent resource decoding do not establish
in-game acceptance. No GitHub remote was supplied; the hosted **Windows tests and
portable application / windows (push)** check has not been run here. Existing
Windows CI and release scripts are retained; no tag or public release was created.
