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
Private assets: the TangTang preset data and the Daki/TangTang rig captures are
not in the repository (private repo `deadlock-mod-builder-private-assets`,
cloned to `private-assets/`, ignored; or `DMB_PRIVATE_ASSETS`). Builds and tests
pass without them (TangTang presets compiled out, nine tests skip with a note);
the Blender harness needs them. See DEVELOPMENT.md "Private assets".
`src-tauri/target` grows to tens of GB; `cargo clean` is safe.

## Repository and release

GitHub repo `OverseerAlbedo/deadlock-mod-builder`, default branch `main`, recreated
on October 15, 2026 with its history purged of third-party data; the previous
repository with the old pull requests and the pre-releases alpha.52 to beta.3 is
`OverseerAlbedo/deadlock-mod-builder-archive` (private). 0.13.0-beta.4 (October 15, 2026) was the first release of the new repository and
0.13.0-beta.5 (Violet), 0.13.0-beta.6 (installed-mods repair), 0.13.0-beta.7 (its Update button
fixed), 0.13.0-beta.8 (automatic-movement fixes), 0.13.0-beta.9 (MMD and Face details)
and 0.13.0-beta.10 (tag `v0.13.0-beta.10`, October 18, 2026, model compiler setup
fixes for issue #2) followed, the latter being the latest; the
repository is public since October 16, 2026. The
repository allows squash and rebase merges only (no merge commits). Releases follow `docs/RELEASING.md`: bump all version
files together, update `CHANGELOG.md` and `docs/RELEASE_NOTES.md`, push, then run
the manual **Release** workflow (`.github/workflows/release.yml`), which tests,
builds the installer, tags the commit and publishes the pre-release. The push
workflow (`.github/workflows/windows.yml`) only builds and tests; it never publishes.

## State as of 0.13.0-beta.10 (October 18, 2026)

Beta 1 = alpha 52 (storage, breast motion, rigid accessories, color editor, Rem
target) plus: the five-step flow with automatic preparation, guns from a
separate file, dual-wield heroes, gun charms, the solid fit preview, textured
gun fixes (UV map order, tint, shared materials), finger curl, short material
names with a compiler path check, live VPK slot state, restyled setup pages,
and the performance pass (bulk mesh export, slim receipts, fewer file reads).

Beta 2 = beta 1 plus: October 2026 game-file fixes (decompiled Softbody cloth
stripped, compiled `$cloth_*` bones listed and drifted bone transforms taken
from the compiled model, KV3 types 26/27), self-checking setup (`setup_checks`,
Test my setup, troubleshooting help on every error), Simple/Expert mode and the
decluttered pages, movement preview with Fine-tune sliders, presets and source
springs, butt/hair/tail motion rules, Shiv/Wraith proportions, autosave,
readable CJK material names, hero picker tags, private logs.

Beta 3 = beta 2 plus: the Theme Manager with Original/Violet D/Mint D and the
customizer (custom presets in settings), the movement preview solver in the
character's frame with calmer tuning and moving parts recorded by every
Analyze, redrawn Fine-tune sliders with Reset all, build freshness after a
build and the VPK slot check at Analyze, the guide in the sidebar, the Blender
preview texture fix, and privacy hardening of local builds. Rust workspace 525
passed / 42 ignored, UI 229 passed, Blender harness 28/28 (with the private
assets; the same Rust and UI counts without them).

Beta 4 = beta 3 plus the private-assets split (TangTang data, rig captures and
the compiled Vindicta references out of the tree; see the rule below) and the
recreated repository. No other user-facing change.

Beta 5 = beta 4 plus Violet: Violet (hero_artist, `models/heroes_wip/artist/artist.vmdl`) was
released in the game on October 16, 2026 (game build 6790) and is playable in
the app; Nurse Harrow follows on October 20. Hero playability now comes from the
game's `m_eHeroDevelopmentState` (`GameTarget::PreRelease`); the roster's
`playable: false` is only the fallback without game data.

Beta 6 = beta 5 plus the repair for the game's October 16, 2026 attachment
change: `deadlock_resource::attachments`, the build taking attachments from
the game's compiled model, and Build & install → Installed mods
(`mod_update.rs`) that rewrites installed VPKs in place. Checked in a match on
October 17, 2026 (Haze, standing and crouching camera right after Update).

Beta 7 = beta 6 with the Update button fixed. Beta 8 = beta 7 plus the
automatic-movement round from SiViolet on Violet: empty hero vertex sets so
Violet's `AE_CL_CLOTH_STIFFEN` events no longer freeze the cloth, gravity in
the game's unit, leg and hip colliders, skirt travel capped at the body,
shorter skirt panels joining the skirt, streamer bend rods and anchor tethers,
Rigid parts not simulated, the Ear motion switch, part-type rules, and the
build's attachment step fixed. Checked in a match on October 17, 2026: Violet
cloth no longer freezes, skirt fixed. Open: floor-length arm cloth ruffles a
bit too much.

Beta 9 = beta 8 plus the MMD round (an MMD version of SiViolet): MMD base
textures instead of sphere maps, automatic texture picks refreshed on
re-preparation, Face details (source alpha-0 layers hidden or shown at build),
head and neck colliders, still hair buns, one-bone parts leaving their helper
bones still, shorter long skirts (0.45/0.6 and panel-by-panel joining) and
long sleeves as streamers. Checked in a match on October 17, 2026: Face details.

Beta 10 = beta 9 plus the model compiler setup fixes from GitHub issue #2: the
Steam QR code read back from DepotDownloader's hidden output and shown on
screen, Download in one DepotDownloader run (one scan) when it reproduces the
plan, Sign in to Steam… for username mode, Steam sign-in error help, wrong
toolset folder messages and the manual download steps page. Checked on the PC
on October 18, 2026: QR sign-in fetched the manifests with one scan.

Test projects on the user's PC (`%USERPROFILE%\OneDrive\Documents\DML_Projects`):
"YvvoneHaze" (Yvonne11.blend on Haze with the Artzy Tyrannical dual gun file
from `Artzy_Tyrannical_Textures_and_Models.zip`), "Ghist" (Lady Geist,
`CH10027_01_Rigged.blend`), "janeVen", "paimonRem".

Not yet checked in game: the finger bend around the gun, the charm swing
amount, dual-gun aim during animations, breast/butt/hair/tail motion amounts
and Fine-tune values, source springs, rigid accessories, color edits and a Rem
build. Checked in game: Shiv and Wraith proportions (October 14). Checked on
the PC: Haze (tangwithgun) and Lady Geist (Ghist) builds compile and restore
the game's cloth with new decompiles; Yamato's new decompile passes the
template check (`tests/cloth_bones.rs`).

## Rules that are easy to break

- Moving-part detection exists three times and must stay identical:
  `deadlock-preparation/src/secondary_bones.rs`, `resources/blender/secondary_bones.py`,
  `src/preparation/secondary-bones.ts`. They share
  `deadlock-preparation/tests/fixtures/hanging-chains.json`; extend that fixture
  and all three tests when the rules change.
- Part kinds (hair, tail, streamer, cloth, breast, butt, charm, rigid) are classified
  separately in `deadlock-cloth-authoring/src/parts.rs` (`semantic`, `analyze`). Its
  names reuse `named_butt`, `named_breast` and `rigid_named` from `secondary_bones.rs`.
- Breast and butt detection (`breast_roots`, `butt_roots`, `facing`, `BUTT_HINTS`) is
  part of the three identical detectors and the shared fixture. `butt_motion` is
  three-way like `breast_motion`: new models get `false` at first inspection
  (`automaticPhysicsOptions`), unset keeps old projects' saved groups, and saved
  butt roots are dropped (not stale) when it is `false`.
- User-facing error help lives in `src-tauri/src/troubleshooting.rs` (rules matched on
  message and details, attached to every `AppError` as `help`); add a test with the
  real message when adding a rule. Settings path checks and fixes are
  `deadlock-tools/src/setup_checks.rs` (`field_checks`, `apply_fixes`); Detect tools
  runs `apply_fixes`. Auto model backend never falls back to the Community SDK.
- Simple/Expert mode is UI-only (`localStorage` `dmb.uiMode`, default Simple). Page
  components take `expert` defaulting to `true`; only `App.tsx` passes the real mode
  (tests run in Expert via `src/test-setup.ts`). Never hide a required choice in Simple.
- Themes (original, violet, mint) are `Settings.theme` (serde default "original"),
  saved at once by the `set_theme` command, which also sets the window icon
  (`src-tauri/icons/icon-{violet,mint}.png`, tauri feature `image-png`). The Theme
  Manager `src/theme/themes.ts` holds each theme as tokens and `applyTheme` writes
  them as CSS variables; custom themes are `Settings.custom_themes` (`ThemePreset`,
  `deriveTokens` from three base colors, editor `ThemeCustomizer.tsx`, saved by
  `save_theme_presets`); the sidebar mark is `BrandMark.tsx`. Never hard-code an
  accent, a tinted grey or a green in CSS: accents use `--accent*`, tinted shades
  are `hsl(calc(var(--hue) ± n) s% l%)`, success stays `--ok*` / `--good*` (not
  themed), errors stay red. Keep text at 4.5:1 in every theme.
- Keep source proportions has a separate path for heroes with extreme proportions
  (`retarget.py hero_extremes`, `scripts/test_hero_proportions.py`); ordinary heroes
  must keep the old numbers exactly (scale 1, floor alignment).
- `scripts/test_blender_moving_parts.py` checks the Python detector against the
  shared fixture (run it with Blender).
- Physics fine-tuning (`secondary_motion.tuning`) is keyed by part **kind**, never
  by root (saved presets must fit any model); it is applied in `generate_inner`
  after `secondary_auto::append` to enabled rows only (`secondary_auto::tune`).
  Travel 0 means free swing and must stay 0.
- The movement preview simulation (`src/preview/motion.ts`) applies Fine-tune with
  `tune`, which must stay identical to Rust `secondary_auto::tune` (both tests use
  the same numbers). Its base values come from Analyze (`Setup.preview_nodes`).
- Automatic motion keeps gravity as a 0–1.5 scale (profiles, `tune`, preview
  nodes, source springs); only `secondary_auto::write_maps` multiplies the
  `cloth_gravity` stream by `GAME_GRAVITY` (360, Valve's compiled unit; the
  compiler copies proxy streams raw). Never scale gravity anywhere else.
  Check compiled values with `deadlock-resource/examples/fe_stats.rs`.
- Leg, hip, neck and head colliders (`body_colliders.rs`) are automatic-motion only, measured
  at Analyze into `Setup.body_colliders` and written by
  `proxy_source::author_direct_proxy`; they must never start with a moving
  proxy node inside them (radius reduced by `CLEARANCE`).
- Rigid parts are not simulated (`Setup.still_roots`, automatic only): no proxy
  points, and every solver bone set comes from `Setup::simulated_groups`, never
  `setup.groups` (the compiled check rejects weighted bones without cloth
  flags). A goal of 1 is not a lock: the compiler caps attraction at 0.5.
- The replaced hero's cloth vertex map names (`Setup.hero_vertex_sets`, read at
  Analyze from the game's compiled model) are written as all-zero
  `cloth_vertex_set_<name>$0` proxy streams, which compile to empty named maps,
  so the hero's `AE_CL_CLOTH_STIFFEN` animation events stiffen nothing (Violet's
  froze the whole cloth on run start/stop). The constraint audit accepts empty
  maps only; never give these sets nodes.
- MMD toon ramps and sphere maps (`mmd_toon_tex`, `mmd_sphere_tex`) are never a
  color source (`material_textures.py MMD_SHADING_NODE`). Preparing the same hero
  again refreshes only automatic picks (`service.rs refreshed_material`, compared
  by prepared file name); never copy old material settings over wholesale.
- Face layers (source alpha 0: blush, tears, other) are marked at preparation
  (`face_layer` on the material) and switched at build in
  `deadlock-materials face_layers.rs` from `Customization.face_layers` (hidden by
  default, shown = alpha mask); never fold them into `Customization.face` (the
  static expression), which is a preparation input.
- Long-skirt limits are `long_skirt::SKIRT_REACH`/`SKIRT_SPAN` (0.45/0.6) and
  `JOINED_REACH`/`JOINED_SPAN` (0.4/0.5, panel by panel through shared mesh).
- Long-skirt travel is capped by each joint's fabric-to-body gap
  (`body_colliders::fabric_gaps`, `body_capped_skirt_travel`) in
  `long_skirt::analyze`; the floor `BODY_TRAVEL_FLOOR` keeps it above 0 (0 = free).
- Streamer chains get post-compile bend rods and anchor tethers (`Setup.strand_bends` →
  `sheet_stability::Plan.strands`, `STRAND_BEND_DEGREES`); anything added to the
  sheet plan must also be in the early-return checks of `generated.rs`
  (`apply_locked_qualified`, `validate_locked_qualified`) and `planned_sheet`.
- Source rig springs (`source_springs.py`, `physics_springs`) change build values
  only when `use_source_springs` is on; keep it off by default.
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
  (`service.rs renamed_materials` carries choices over by it). `prepared_slug`
  spells non-English names in Latin letters (`readable_names.py`,
  `cjk_readings.txt`); ASCII names must keep their old slug byte for byte
  (`scripts/test_readable_names.py`).
- `.gitattributes` keeps fixture bytes exact; do not reformat fixture JSON.
- Attachments of a compiled model live in every `MDAT` block (`m_attachments`).
  Since the game's October 16, 2026 update, attachments on the root bone
  (`root_motion`) are stored without the bone's rest rotation (new = rest
  rotation × old; other bones unchanged). `deadlock_resource::attachments`
  classifies a model against the game's current model (`Convention`) and
  rewrites it; the build takes the game's attachments after compiling
  (`build.rs`), and `deadlock_pipeline::mod_update` scans and repairs installed
  VPKs in place (Build & install → Installed mods). Never bake attachment
  values into fixtures from game models.
- DepotDownloader runs hidden (no console, stdin null), so it can never prompt.
  QR sign-in: `deadlock_tools::steam_qr::read` reads the code back from its
  output by layout only (the console code page's block bytes arrive as U+FFFD),
  `SteamQrWatcher` (app.rs, QR mode of `setup_acquire` only) sends it as the
  `steam-qr` event and `SteamQrDialog.tsx` shows it. Username mode needs a saved
  sign-in from Sign in to Steam… (`setup_steam_sign_in`: a visible console via
  `cmd /v:on /c start /wait … & exit !errorlevel!`; plain `start /wait` loses
  the exit code).
- Download all depots in one DepotDownloader run (one QR scan) only when
  `acquisition::single_run_filelist` shows it writes exactly the planned files;
  keep the per-depot fallback (`acquisition::download_commands`, used by the
  app and the manual guide). Never add `-remember-password` to `Auth::Qr`; a
  `-qr` run always starts a new QR session, so it is one scan per run.
- The manual download guide exists twice and must say the same steps: plain
  text in `manual_setup.rs guide` (instructions.txt) and the page
  `src/setup/ManualGuide.tsx`.
- Kept source bones (custom bones next to the hero's restored cloth, Preserve
  original physics) get the same animation-rest reconciliation as generated
  bones before any source-binding check (`restore.rs kept_binds` →
  `bind_repair::prepare_kept`, run before the alias repair); the compiler
  rounds their DATA locals down long chains. Never widen the transfer
  budgets instead. `tests/kept_bones.rs` (`DMB_KEPT_BONES`) and
  `examples/bone_drift.rs` in deadlock-cloth-authoring measure it.
- The full-precision cloth reader (`tools/cloth-reader`, bundled in
  `resources/native/cloth-reader`) is a second, independent KV3 parser
  (ValveResourceFormat). When Valve adds KV3 types, the Rust decoder
  (`deadlock-resource binary.rs`) and the reader both need them; rebuild the
  reader per its README and run `tests/motion.rs
  current_game_models_pass_the_independent_reader` with `DMB_GAME_MODELS` set
  to extracted hero models. Builds that keep the hero's own cloth are the ones
  that feed it Valve-compiled data.
- Hero colors (Extras → Colors, `Customization.hero_materials`,
  `deadlock-components materials.rs`) edit the hero's own compiled `.vmat_c` in
  place: only existing `m_vectorParams` values (`g_vColorTint1`, and y/z of
  `g_vAlbedoContrastSaturationBrightness1`), `pbr.vfx` only, every non-DATA block
  byte exact. Never add a parameter or decompile/recompile game materials. They
  are refused with a replacement model (a separate mode); the asset-only package
  check counts `.vmat_c` as extras only then (`build.rs component_path`).
- Hero color ranges (`HeroMaterial.ranges`) recolor a copy of the game's BC7
  `g_tColor`: the pixel function exists twice and must stay identical,
  `deadlock-components color_ranges.rs` and `src/extras/heroColorRanges.ts`
  (shared fixture `deadlock-components/tests/fixtures/hero-color-ranges.json`).
  `texture_recolor.rs` recolors every mip and the BC3 fallback from their own
  texels and refits only changed blocks (`block_refit.rs`, keeping every bit that
  feeds alpha, so metalness stays byte exact); never add a fresh encoder, a mip
  filter or an encoder dependency. The copy lives under
  `materials/dmb_hero_colors/`, the material's `g_tColor` and RERL name it
  (`deadlock_vpk::resource_id`). It is a packaged file, not a game file:
  `service.rs packaged()` lists it, `targets()`, the closure and original hashes
  keep game files only, and install/package checks take it from the receipt
  (`materials::packaged_textures`).
- Hero tags come from the installed game's definitions (`heroes.rs GameModels`:
  `m_bDisabled` → In development, `m_eHeroDevelopmentState` PreRelease → Not
  playable yet, else In game); `hero_roster.json` `playable: false` is only the
  fallback when the definitions cannot be read. Do not hand-flag released heroes.
- Anything captured from a third-party character (TangTang proxies and groups,
  Daki/TangTang rig JSON, the anti-twist capture) and the compiled Vindicta
  references go to `private-assets/`, never into the repository
  (`scripts/sync-private-assets.mjs` copies the references into
  `physics-references/`, gitignored, for a local build; the Physics page hides
  the included reference when `get_capabilities` says it is absent).
  `deadlock-cloth-authoring/build.rs` sets `cfg(tangtang_assets)` when the
  folder is there; code reads the data through `tangtang_assets::GROUPS` /
  `R2_PROXY` / `REFERENCE_PROXY` (`Option`) and must work when they are `None`.
  Tests load it with `tests/common/mod.rs` `private_text` and return early
  when it is missing.
- Do not update dependencies or lockfiles unless the change needs it.
- Version must agree in package.json, package-lock.json, Cargo.toml files and
  tauri.conf.json (`node scripts/check-repository.mjs` checks this).
