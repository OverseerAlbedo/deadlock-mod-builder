# Extras icon discovery and roster audit

Alpha 51 discovers images by combining known hero names with explicit references in the installed game. The model selects a hero definition, and only that hero's reachable abilities and child abilities contribute references. Existing texture variants remain visible. Shared names require explicit replacement consent; vector resources are preview-only.

## Why Abrams and Lady Geist were incomplete

The previous scan required a hero-name token inside each compiled texture path. Abrams' portrait family is `bull`, and Lady Geist's is `spectre`. Those aliases were missing. This release adds these and the corresponding `sumo`, `kali`, `giga` and `grappler` families, follows game references for unnamed/shared assets, and includes compiled vector logos.

## Verified inventory

Source: [e6bb7e33d212](https://github.com/SteamTracking/GameTracking-Deadlock/commit/e6bb7e33d212b110d138f98427ef37f9a80c2839), October 11, 2026. Full-file Git blob identities were verified against the repository tree. The production Rust parser read the complete decompiled hero and ability tables and the game archive index. All 47 app roster entries returned images, and no available explicit image reference was omitted. The index contains 2,742 compiled Panorama image resources.

Counts include name-based alternate artwork and shared images referenced by the selected hero. Textures are discovered resources, not a promise that every texture layout is replaceable. Vector resources are preview-only.

| Character / model | Textures | Vectors | Available references omitted | References absent in game |
| --- | ---: | ---: | ---: | ---: |
| Abrams | 18 | 3 | 0 | 0 |
| Apollo | 18 | 3 | 0 | 0 |
| Baba | 27 | 3 | 0 | 0 |
| Bebop | 17 | 3 | 0 | 0 |
| Billy | 20 | 3 | 0 | 0 |
| Calico | 22 | 3 | 0 | 0 |
| Celeste | 19 | 3 | 0 | 0 |
| Deadman Danny | 9 | 3 | 0 | 1 |
| Doorman | 23 | 3 | 0 | 0 |
| Drifter | 22 | 3 | 0 | 0 |
| Dynamo | 18 | 3 | 0 | 0 |
| Graves | 20 | 4 | 0 | 0 |
| Grey Talon | 18 | 3 | 0 | 0 |
| Haze | 18 | 3 | 0 | 0 |
| Holliday | 19 | 3 | 0 | 0 |
| Infernus | 18 | 3 | 0 | 0 |
| Ivy | 17 | 3 | 0 | 0 |
| Kelvin | 17 | 3 | 0 | 0 |
| Lady Geist | 19 | 5 | 0 | 0 |
| Lash | 17 | 3 | 0 | 0 |
| McGinnis | 19 | 4 | 0 | 0 |
| Mina | 20 | 3 | 0 | 0 |
| Mirage | 19 | 3 | 0 | 0 |
| Mo & Krill | 17 | 3 | 0 | 0 |
| Nurse Harrow | 9 | 3 | 0 | 1 |
| Paige | 19 | 3 | 0 | 0 |
| Paradox | 17 | 3 | 0 | 0 |
| Pocket | 18 | 3 | 0 | 0 |
| Rem | 17 | 4 | 0 | 0 |
| Rem (unused model, `familiar.vmdl`) | 17 | 4 | 0 | 0 |
| Seven | 17 | 3 | 0 | 0 |
| Shiv | 23 | 3 | 0 | 0 |
| Shiv Ult | 23 | 3 | 0 | 0 |
| Silver | 23 | 3 | 0 | 0 |
| Silver Ult | 23 | 3 | 0 | 0 |
| Sinclair | 20 | 3 | 0 | 0 |
| Solomon | 25 | 3 | 0 | 0 |
| The Rat King | 20 | 5 | 0 | 0 |
| Venator | 18 | 3 | 0 | 0 |
| Victor | 20 | 3 | 0 | 0 |
| Vindicta | 17 | 3 | 0 | 0 |
| Violet | 9 | 3 | 0 | 1 |
| Viscous | 18 | 3 | 0 | 0 |
| Vyper | 22 | 3 | 0 | 0 |
| Warden | 17 | 3 | 0 | 0 |
| Wraith | 18 | 3 | 0 | 0 |
| Yamato | 17 | 3 | 0 | 0 |

Three referenced portraits are absent from the source game inventory:

- Deadman Danny: `panorama/images/heroes/deadpack_card_psd.vtex_c`
- Nurse Harrow: `panorama/images/heroes/nurse_card_psd.vtex_c`
- Violet: `panorama/images/heroes/artist_card_psd.vtex_c`

These missing files produce notices. The app does not invent archive destinations or remove other available images. The per-character path list, shared classification, source hashes and URLs are in `docs/evidence/extras-icon-inventory.json`.

## Reproduce

From the repository root, provide complete decompiled `heroes.vdata`, `abilities.vdata` and a `pak01_dir.txt` index from the same game snapshot:

```sh
cargo run --manifest-path src-tauri/Cargo.toml -p deadlock-components --example audit_icons -- heroes.vdata abilities.vdata pak01_dir.txt icon-audit.json
python3 scripts/collect_icon_inventory.py heroes.vdata abilities.vdata pak01_dir.txt src-tauri/crates/deadlock-components/tests/fixtures/icons/game-image-inventory.json
cargo test --manifest-path src-tauri/Cargo.toml -p deadlock-components --test icon_catalog
```

The committed inventory fixture retains model identities, image filenames and ability links, not artwork. It was extracted independently of the Rust implementation. The regression suite checks binary and text definitions, known legacy aliases, unnamed assets, hash variants, child/cyclic ability references, shared-resource consent, ambiguous prototype ownership, unsafe paths, malformed definitions and vector previews.

Optional real-format fixtures can be checked by setting `DMB_ICON_NATIVE_FIXTURES` to a directory containing `pip-left.vsvg_c` and `abilities_kv3_v5_zstd.vdata_c` from [ValveResourceFormat Tests/Files](https://github.com/ValveResourceFormat/ValveResourceFormat/tree/master/Tests/Files). Both were used for this release.

## Scope

This audit checks filenames and discovery against a real game snapshot. It does not open every game image payload, run the Windows app, or verify in-game appearance. Unknown texture formats remain visible with their existing support explanation. Runtime discovery reads the user's installed game instead of shipping these snapshot filenames as a fixed catalog. The Alpha 50 preparation, skeleton, cloth, reference and related UI/test files are unchanged; hashes are in `docs/evidence/preserved-alpha50-files.json`.
