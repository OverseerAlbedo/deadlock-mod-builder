# Accepted Vindicta balanced reference

This is the exact Balanced Test 2 resource accepted by the user on October 5, 2026. The application's included Vindicta cloth action selects it for the matching hero/model.

SHA-256: `27dd44ea4db29f671cff53d0dd8ad171792fe5f12f45b05aca5db65e0a6b5b0c`

Keep the matching 95-bone prepared replacement selected, then Analyze and Build. Only the supported complete physics solver is transferred; reference meshes and animation samples are not copied into the replacement. This release applies no further tuning to the reference. `baseline.json` preserves the original acceptance record; its historical evidence path refers to the prior development archive.

The original R12 file remains in `../Vindicta-R12/` for regression tests and explicit rollback. The reference does not authorize another hero, changed rest pose or unrelated cloth topology. Build/install integrity checks remain active.

Ownership and distribution scope are described in the root LICENSE and `docs/THIRD_PARTY.md`; this compiled game-derived resource is not licensed as application source.

The compiled file itself is a private asset (private repository `deadlock-mod-builder-private-assets`, folder `physics-references/Vindicta-R12-Balanced/`); `scripts/sync-private-assets.mjs` copies it here for a local build, and the Physics page hides the included reference when it is absent.
